use super::receive;
use crate::graph_store::graph::Graph;
use crate::load::load_strategy::LoadStrategy;
use crate::arangodb::aql::get_all_data_aql;
use crate::arangodb::dump::{CollectionsInfo, compute_shard_infos, get_all_shard_data, ShardDistribution};
use crate::arangodb::{handle_arangodb_response_with_parsed_body};
use crate::arangodb::info::{DeploymentType, SupportInfo, VersionInformation};
use crate::client::auth::handle_auth;
use crate::client::build_client;
use crate::client::config::ClientConfig;
use crate::input::load_request::{DataLoadRequest, DatabaseConfiguration, Progress, CollectionDescription};
use bytes::Bytes;
use log::{error, info};
use reqwest::StatusCode;
use std::error::Error;
use std::num::ParseIntError;
use std::sync::{Arc, Mutex, RwLock};
use std::thread::JoinHandle;
use std::time::SystemTime;
use reqwest_middleware::ClientWithMiddleware;

pub fn get_arangodb_graph(req: DataLoadRequest) -> Result<Graph, String> {
    let graph = Arc::new(RwLock::new(Graph::new(
        true,
        req.vertex_attributes.clone(),
    )));

    let graph_clone = graph.clone(); // for background thread
    println!("Starting computation");
    let prog_arg = Arc::new(RwLock::new(Progress::new(3)));
    // Fetch from ArangoDB in a background thread:
    let handle = std::thread::spawn(move || {
        tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(async {
                println!("Loading!");
                fetch_graph_from_arangodb(req, graph_clone, prog_arg.clone()).await
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

async fn fetch_edge_and_vertex_collections_by_graph(
    client: &ClientWithMiddleware,
    db_config: &DatabaseConfiguration,
    url: String,
) -> Result<(Vec<String>, Vec<String>), String> {
    let mut edge_collection_names = vec![];
    let mut vertex_collection_names = vec![];

    let resp = handle_auth(client.get(url), db_config).send().await;
    let parsed_response =
        handle_arangodb_response_with_parsed_body::<serde_json::Value>(resp, StatusCode::OK)
            .await?;
    let graph = parsed_response["graph"]
        .as_object()
        .ok_or("graph is not an object")?;
    let edge_definitions = graph
        .get("edgeDefinitions")
        .ok_or("no edgeDefinitions")?
        .as_array()
        .ok_or("edgeDefinitions is not an array")?;

    let mut non_unique_vertex_collection_names = vec![];
    for edge_definition in edge_definitions {
        let edge_collection_name = edge_definition["collection"]
            .as_str()
            .ok_or("collection is not a string")?;
        edge_collection_names.push(edge_collection_name.to_string());

        let from = edge_definition["from"]
            .as_array()
            .ok_or("from is not an array")?;
        for vertex in from {
            let vertex_collection_name =
                vertex.as_str().ok_or("from collection is not a string")?;
            non_unique_vertex_collection_names.push(vertex_collection_name.to_string());
        }

        let to = edge_definition["to"]
            .as_array()
            .ok_or("to is not an array")?;
        for vertex in to {
            let vertex_collection_name = vertex.as_str().ok_or("to collection is not a string")?;
            non_unique_vertex_collection_names.push(vertex_collection_name.to_string());
        }
    }

    non_unique_vertex_collection_names.sort();
    non_unique_vertex_collection_names.dedup();
    vertex_collection_names.append(&mut non_unique_vertex_collection_names);

    Ok((vertex_collection_names, edge_collection_names))
}

async fn get_graph_collections(
    url: String,
    graph_name: Option<String>,
    mut vertex_collections: Vec<CollectionDescription>,
    mut edge_collections: Vec<CollectionDescription>,
    client: &ClientWithMiddleware,
    db_config: &DatabaseConfiguration,
    begin: SystemTime,
) -> Result<(Vec<CollectionDescription>, Vec<CollectionDescription>), String> {
    match &graph_name {
        Some(name) if !name.is_empty() => {
            if !vertex_collections.is_empty() || !edge_collections.is_empty() {
                let error_message =
                    "Either specify the graph_name or ensure that vertex_collections and edge_collections are not empty.";
                error!("{:?}", error_message);
                return Err(error_message.to_string());
            }

            // in case a graph name has been give, we need to fetch the vertex and edge collections from ArangoDB
            let (vertices, edges) =
                fetch_edge_and_vertex_collections_by_graph(client, db_config, url).await?;
            info!(
            "{:?} Got vertex collections: {:?}, edge collections: {:?} from graph definition for: {:?}.",
            SystemTime::now().duration_since(begin).unwrap(),
            vertices, edges, name
        );
            for vertex in vertices {
                vertex_collections.push(CollectionDescription {
                    name: vertex,
                    fields: vec![],
                });
            }
            for edge in edges {
                edge_collections.push(CollectionDescription {
                    name: edge,
                    fields: vec![],
                });
            }
        }
        Some(_) => {
            return Err("Graph name is set but it's empty.".to_string());
        }
        _ => {
            if vertex_collections.is_empty() || edge_collections.is_empty() {
                let error_message =
                    "Either specify the graph_name or ensure that vertex_collections and edge_collections are not empty.";
                error!("{:?}", error_message);
                return Err(error_message.to_string());
            }
        }
    }

    Ok((vertex_collections, edge_collections))
}

pub async fn get_arangodb_environment(client: &ClientWithMiddleware, db_config: &DatabaseConfiguration) -> Result<(LoadStrategy, SupportInfo), String> {
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

    Ok((load_strategy, support_info))
}

pub async fn fetch_graph_from_arangodb(
    req: DataLoadRequest,
    graph_arc: Arc<RwLock<Graph>>,
    prog_arc: Arc<RwLock<Progress>>,
) -> Result<Arc<RwLock<Graph>>, String> {
    let db_config = &req.configuration.database_config;
    if db_config.endpoints.is_empty() {
        return Err("no endpoints given".to_string());
    }
    let begin = prog_arc.read().unwrap().get_start_time();

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

    let (load_strategy, support_info) = get_arangodb_environment(&client, &req.configuration.database_config).await?;

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
    let deployment_type = support_info.deployment.deployment_type.clone();

    let vertex_collections;
    let edge_collections;

    let named_graph_string = format!("/_api/gharial/{:?}", req.graph_name);
    let named_graph_url = make_url(&named_graph_string);
    match get_graph_collections(
        named_graph_url,
        req.graph_name.clone(),
        req.vertex_collections.clone(),
        req.edge_collections.clone(),
        &client,
        db_config,
        begin,
    )
        .await
    {
        Ok((v_cols, e_cols)) => {
            vertex_collections = v_cols;
            edge_collections = e_cols;
        }
        Err(err) => {
            return Err(err);
        }
    }

    // Compute which shard we must get from which dbserver, we do vertices
    // and edges right away to be able to error out early:
    let vertices_infos = compute_shard_infos(
        &shard_dist,
        vertex_collections,
        &deployment_type,
        &db_config.endpoints,
    )?;
    if vertices_infos.no_shards_found() {
        error!("No vertex shards found!");
        return Err("No vertex shards found!".to_string());
    }
    {
        let mut progress = prog_arc.write().unwrap();
        match progress.set_current_state("Now loading vertices".to_string()) {
            Ok(_) => (),
            Err(e) => error!("Error: {}", e),
        }
    }

    load_vertices(
        &req,
        &graph_arc,
        &db_config,
        &support_info,
        begin,
        vertices_infos,
        load_strategy,
    )
        .await?;
    {
        let mut progress = prog_arc.write().unwrap();
        match progress.set_current_state("Vertices loaded. Now continuing with edges.".to_string()) {
            Ok(_) => (),
            Err(e) => error!("Error: {}", e),
        }
    }

    // And now the edges:
    let edges_infos = compute_shard_infos(
        &shard_dist,
        edge_collections,
        &deployment_type,
        &db_config.endpoints,
    )?;
    if edges_infos.no_shards_found() {
        error!("No edge shards found!");
        return Err("No edge shards found!".to_string());
    }

    load_edges(
        &req,
        &graph_arc,
        &db_config,
        &support_info,
        begin,
        edges_infos,
        &load_strategy,
    )
        .await?;
    {
        let mut progress = prog_arc.write().unwrap();
        match progress.set_current_state("Graph fully loaded.".to_string()) {
            Ok(_) => (),
            Err(e) => error!("Error: {}", e),
        }
    }


    {
        info!(
            "{:?} Graph loaded.",
            std::time::SystemTime::now().duration_since(begin).unwrap()
        );
    }
    Ok(graph_arc)
}

async fn load_edges(
    req: &DataLoadRequest,
    graph_arc: &Arc<RwLock<Graph>>,
    db_config: &&DatabaseConfiguration,
    support_info: &SupportInfo,
    begin: SystemTime,
    collection_infos: CollectionsInfo,
    load_strategy: &LoadStrategy,
) -> Result<(), String> {
    let mut senders: Vec<std::sync::mpsc::Sender<Bytes>> = vec![];
    let mut consumers: Vec<JoinHandle<Result<(), String>>> = vec![];
    let prog_reported = Arc::new(Mutex::new(0_u64));

    for _i in 0..req
        .configuration
        .parallelism
        .expect("Why is parallelism missing")
    {
        let prog_reported_clone = prog_reported.clone();
        let (sender, receiver) = std::sync::mpsc::channel::<Bytes>();
        senders.push(sender);
        let graph_clone = graph_arc.clone();
        let load_strategy_clone = *load_strategy;
        let consumer = std::thread::spawn(move || {
            receive::receive_edges(receiver, graph_clone, load_strategy_clone, prog_reported_clone, begin)
        });
        consumers.push(consumer);
    }
    match load_strategy {
        LoadStrategy::Dump => {
            get_all_shard_data(req, db_config, &support_info, collection_infos, senders, true).await?;
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
    support_info: &SupportInfo,
    begin: SystemTime,
    collection_infos: CollectionsInfo,
    load_strategy: LoadStrategy,
) -> Result<(), String> {
    // We use multiple threads to receive the data in batches:
    let mut senders: Vec<std::sync::mpsc::Sender<Bytes>> = vec![];
    let mut consumers: Vec<JoinHandle<Result<(), String>>> = vec![];
    let prog_reported = Arc::new(Mutex::new(0_u64));

    for _i in 0..req
        .configuration
        .parallelism
        .expect("Why is parallelism missing")
    {
        let collection_infos_thread = collection_infos.clone();
        let (sender, receiver) = std::sync::mpsc::channel::<Bytes>();
        senders.push(sender);
        let graph_clone = graph_arc.clone();
        let prog_reported_clone = prog_reported.clone();
        let load_strategy_clone = load_strategy;
        let consumer = std::thread::spawn(move || {
            receive::receive_vertices(
                receiver,
                graph_clone,
                collection_infos_thread,
                load_strategy_clone,
                prog_reported_clone,
                begin,
            )
        });
        consumers.push(consumer);
    }
    match load_strategy {
        LoadStrategy::Dump => {
            get_all_shard_data(req, db_config, support_info, collection_infos, senders, false).await?;
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
