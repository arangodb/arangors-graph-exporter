use super::receive;
use crate::graphs::Graph;
use crate::load::load_strategy::LoadStrategy;
use crate::arangodb::aql::get_all_data_aql;
use crate::arangodb::dump::{
    compute_shard_map, get_all_shard_data, ShardDistribution, ShardMap,
};
use crate::arangodb::handle_arangodb_response_with_parsed_body;
use crate::arangodb::info::{DeploymentType, SupportInfo, VersionInformation};
use crate::client::auth::handle_auth;
use crate::client::build_client;
use crate::client::config::ClientConfig;
use crate::input::load_request::{DataLoadRequest, DatabaseConfiguration};
use bytes::Bytes;
use log::info;
use reqwest::StatusCode;
use std::collections::HashMap;
use std::error::Error;
use std::num::ParseIntError;
use std::sync::{Arc, RwLock};
use std::thread::JoinHandle;
use std::time::SystemTime;

pub fn get_arangodb_graph(req: DataLoadRequest) -> Result<Graph, String> {
    let graph = Graph::new(true, 64, 0);
    let graph_clone = graph.clone(); // for background thread
    println!("Starting computation");
    // Fetch from ArangoDB in a background thread:
    let handle = std::thread::spawn(move || {
        tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(async {
                println!("Loading!");
                fetch_graph_from_arangodb(req, graph_clone).await
            })
    });
    handle.join().map_err(|_s| "Computation failed")??;
    let inner_rw_lock =
        Arc::<std::sync::RwLock<Graph>>::try_unwrap(graph).map_err(|poisoned_arc| {
            if poisoned_arc.is_poisoned() {
                "Computation failed: thread failed - poisoned arc"
            } else {
                "Computation failed"
            }
        })?;
    inner_rw_lock.into_inner().map_err(|poisoned_lock| {
        format!(
            "Computation failed: thread failed - poisoned lock {}",
            poisoned_lock
                .source()
                .map_or(String::from(""), <dyn Error>::to_string)
        )
    })
}

pub async fn fetch_graph_from_arangodb(
    req: DataLoadRequest,
    graph_arc: Arc<RwLock<Graph>>,
) -> Result<Arc<RwLock<Graph>>, String> {
    let db_config = &req.configuration.database_config;
    if db_config.endpoints.is_empty() {
        return Err("no endpoints given".to_string());
    }
    let begin = std::time::SystemTime::now();

    println!(
        "{:?} Fetching graph from ArangoDB...",
        std::time::SystemTime::now().duration_since(begin).unwrap()
    );

    let use_tls = db_config.endpoints[0].starts_with("https://");
    let client_config = ClientConfig::builder()
        .n_retries(5)
        .use_tls(use_tls)
        .tls_cert_opt(db_config.tls_cert.clone())
        .build();
    let client = build_client(&client_config)?;

    let server_version_url = db_config.endpoints[0].clone() + "/_api/version";
    let resp = handle_auth(client.get(server_version_url), db_config)
        .send()
        .await;
    let version_info =
        handle_arangodb_response_with_parsed_body::<VersionInformation>(resp, StatusCode::OK)
            .await?;

    static MIN_SUPPORTED_MINOR_VERSIONS: &[(u8, u8)] = &[(3, 12)];
    let version_parts: Vec<&str> = version_info.version.split('.').collect();
    if version_parts.len() < 3 {
        return Err(format!(
            "Unable to parse ArangoDB Version - got {}",
            version_info.version
        ));
    }

    let supports_v1 = {
        let major: u8 = version_parts
            .first()
            .ok_or("Unable to parse Major Version".to_string())?
            .parse()
            .map_err(|err: ParseIntError| err.to_string())?;
        let minor: u8 = version_parts
            .get(1)
            .ok_or("Unable to parse Minor Version".to_string())?
            .parse()
            .map_err(|err: ParseIntError| err.to_string())?;
        let major_supports = MIN_SUPPORTED_MINOR_VERSIONS
            .iter()
            .map(|x| x.0)
            .any(|x| x == major);
        if !major_supports {
            false
        } else {
            MIN_SUPPORTED_MINOR_VERSIONS
                .iter()
                .find(|x| x.0 == major)
                .ok_or("Unable to find supported version".to_string())?
                .1
                <= minor
        }
    };

    let server_information_url = db_config.endpoints[0].clone() + "/_admin/support-info";
    let support_info_res = handle_auth(client.get(server_information_url), db_config)
        .send()
        .await;
    let support_info =
        handle_arangodb_response_with_parsed_body::<SupportInfo>(support_info_res, StatusCode::OK)
            .await?;

    let load_strategy =
        if !supports_v1 && support_info.deployment.deployment_type == DeploymentType::Single {
            LoadStrategy::Aql
        } else {
            LoadStrategy::Dump
        };

    let make_url =
        |path: &str| -> String { db_config.endpoints[0].clone() + "/_db/" + &req.database + path };

    // First ask for the shard distribution:
    let url = make_url("/_admin/cluster/shardDistribution");
    let resp = handle_auth(client.get(url), db_config).send().await;
    let shard_dist = match support_info.deployment.deployment_type {
        DeploymentType::Single => None,
        DeploymentType::Cluster => {
            let shard_dist = handle_arangodb_response_with_parsed_body::<ShardDistribution>(
                resp,
                StatusCode::OK,
            )
            .await?;
            Some(shard_dist)
        }
    };
    let deployment_type = support_info.deployment.deployment_type;

    // Compute which shard we must get from which dbserver, we do vertices
    // and edges right away to be able to error out early:
    let vertex_coll_list = req
        .vertex_collections
        .iter()
        .map(|ci| -> String { ci.name.clone() })
        .collect::<Vec<String>>();
    let vertex_map = compute_shard_map(
        &shard_dist,
        &vertex_coll_list,
        &deployment_type,
        &db_config.endpoints,
    )?;
    let vertex_coll_field_map: Arc<RwLock<HashMap<String, Vec<String>>>> =
        Arc::new(RwLock::new(HashMap::new()));
    {
        let mut guard = vertex_coll_field_map.write().unwrap();
        for vc in req.vertex_collections.iter() {
            guard.insert(vc.name.clone(), vc.fields.clone());
        }
    }

    info!(
        "{:?} Need to fetch data from {} vertex shards...",
        std::time::SystemTime::now().duration_since(begin).unwrap(),
        vertex_map.values().map(|v| v.len()).sum::<usize>(),
    );

    load_vertices(
        &req,
        &graph_arc,
        &db_config,
        begin,
        &vertex_map,
        vertex_coll_field_map,
        load_strategy,
    )
    .await?;

    if !req.edge_collections.is_empty() {
        let edge_coll_list = req
            .edge_collections
            .iter()
            .map(|ci| -> String { ci.name.clone() })
            .collect::<Vec<String>>();
        let edge_map = compute_shard_map(
            &shard_dist,
            &edge_coll_list,
            &deployment_type,
            &db_config.endpoints,
        )?;

        info!(
            "{:?} Need to fetch data from {} edge shards...",
            std::time::SystemTime::now().duration_since(begin).unwrap(),
            edge_map.values().map(|v| v.len()).sum::<usize>()
        );

        load_edges(
            &req,
            &graph_arc,
            &db_config,
            begin,
            &edge_map,
            &load_strategy,
        )
        .await?;
    }

    // And now the edges:
    {
        info!(
            "{:?} Graph loaded.",
            std::time::SystemTime::now().duration_since(begin).unwrap()
        );
    }
    info!("hi");
    Ok(graph_arc)
}

async fn load_edges(
    req: &DataLoadRequest,
    graph_arc: &Arc<RwLock<Graph>>,
    db_config: &&DatabaseConfiguration,
    begin: SystemTime,
    edge_map: &ShardMap,
    load_strategy: &LoadStrategy,
) -> Result<(), String> {
    info!("loading edges");
    let mut senders: Vec<std::sync::mpsc::Sender<Bytes>> = vec![];
    let mut consumers: Vec<JoinHandle<Result<(), String>>> = vec![];
    for _i in 0..req
        .configuration
        .parallelism
        .expect("Why is parallelism missing")
    {
        let (sender, receiver) = std::sync::mpsc::channel::<Bytes>();
        senders.push(sender);
        let graph_clone = graph_arc.clone();
        let load_strategy_clone = *load_strategy;
        let consumer = std::thread::spawn(move || {
            receive::receive_edges(receiver, graph_clone, load_strategy_clone)
        });
        consumers.push(consumer);
    }
    match load_strategy {
        LoadStrategy::Dump => {
            get_all_shard_data(req, db_config, edge_map, senders).await?;
        }
        LoadStrategy::Aql => {
            get_all_data_aql(req, db_config, &req.edge_collections, senders, true).await?;
        }
    }
    info!(
        "{:?} Got all data, processing...",
        std::time::SystemTime::now().duration_since(begin).unwrap()
    );
    for c in consumers {
        let _guck = c.join();
    }
    Ok(())
}

async fn load_vertices(
    req: &DataLoadRequest,
    graph_arc: &Arc<RwLock<Graph>>,
    db_config: &&DatabaseConfiguration,
    begin: SystemTime,
    vertex_map: &ShardMap,
    vertex_coll_field_map: Arc<RwLock<HashMap<String, Vec<String>>>>,
    load_strategy: LoadStrategy,
) -> Result<(), String> {
    info!("loading vertices");
    // We use multiple threads to receive the data in batches:
    let mut senders: Vec<std::sync::mpsc::Sender<Bytes>> = vec![];
    let mut consumers: Vec<JoinHandle<Result<(), String>>> = vec![];
    for _i in 0..req
        .configuration
        .parallelism
        .expect("Why is parallelism missing")
    {
        let (sender, receiver) = std::sync::mpsc::channel::<Bytes>();
        senders.push(sender);
        let graph_clone = graph_arc.clone();
        let vertex_coll_field_map_clone = vertex_coll_field_map.clone();
        let load_strategy_clone = load_strategy;
        let consumer = std::thread::spawn(move || {
            receive::receive_vertices(
                receiver,
                graph_clone,
                vertex_coll_field_map_clone,
                load_strategy_clone,
            )
        });
        consumers.push(consumer);
    }
    match load_strategy {
        LoadStrategy::Dump => {
            get_all_shard_data(req, db_config, vertex_map, senders).await?;
        }
        LoadStrategy::Aql => {
            get_all_data_aql(req, db_config, &req.vertex_collections, senders, false).await?;
        }
    }
    info!(
        "{:?} Got all data, processing...",
        std::time::SystemTime::now().duration_since(begin).unwrap()
    );
    for c in consumers {
        let _guck = c.join();
    }
    Ok(())
}
