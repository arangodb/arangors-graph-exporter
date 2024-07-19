use crate::client::auth::handle_auth;
use crate::client::config::ClientConfig;
use crate::client::{build_client, make_url};
use crate::errors::GraphLoaderError;
use crate::request::handle_arangodb_response;
use crate::types::info::DeploymentType;
use crate::{errors, DataLoadConfiguration, DatabaseConfiguration};
use bytes::Bytes;
use log::{debug, error};
use reqwest::StatusCode;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::time::SystemTime;
use tokio::task::JoinSet;

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct CollectionDistribution {
    plan: HashMap<String, ShardLocation>,
    current: HashMap<String, ShardLocation>,
}

#[derive(Debug, Clone)]
struct DBServerInfo {
    dbserver: String,
    dump_id: String,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
struct DumpStartBody {
    batch_size: u64,
    prefetch_count: u32,
    parallelism: u32,
    shards: Vec<String>,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
struct DumpStartBodyWithProjections {
    batch_size: u64,
    prefetch_count: u32,
    parallelism: u32,
    shards: Vec<String>,
    projections: Option<HashMap<String, Vec<String>>>,
}

#[derive(Debug, Serialize, Deserialize)]
struct ShardLocation {
    leader: String,
    followers: Vec<String>,
}
#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct ShardDistribution {
    error: bool,
    code: i32,
    results: HashMap<String, CollectionDistribution>,
}

// A ShardMap maps dbserver names to lists of shards for which these dbservers
// are leaders. We will have one for the vertices and one for the edges.
pub(crate) type ShardMap = HashMap<String, Vec<String>>;

pub(crate) async fn get_all_shard_data(
    db_config: &DatabaseConfiguration,
    load_config: &DataLoadConfiguration,
    shard_map: &ShardMap,
    result_channels: Vec<tokio::sync::mpsc::Sender<Bytes>>,
    deployment_type: &DeploymentType,
    projections: Option<HashMap<String, Vec<String>>>,
) -> Result<(), GraphLoaderError> {
    let use_tls = db_config.endpoints[0].starts_with("https://");
    let client_config = ClientConfig::builder()
        .n_retries(5)
        .use_tls(use_tls)
        .tls_cert_opt(db_config.tls_cert.clone())
        .build();
    let client = build_client(&client_config)?;

    // Start a single dump context on all involved dbservers, we can do
    // this sequentially, since it is not performance critical, we can
    // also use the same HTTP client and the same first endpoint:
    let mut dbservers: Vec<DBServerInfo> = vec![];
    let mut error_happened = false;
    let mut error: String = "".into();
    for (server, shard_list) in shard_map.iter() {
        let url = if deployment_type == DeploymentType::Cluster {
            make_url(db_config, &format!("/_api/dump/start?dbserver={}", server))
        } else {
            make_url(db_config, "/_api/dump/start")
        };

        let body_v: Vec<u8>;
        if projections.is_some() {
            let body = DumpStartBodyWithProjections {
                batch_size: load_config.batch_size,
                prefetch_count: load_config.prefetch_count,
                parallelism: load_config.parallelism,
                shards: shard_list.clone(), // in case of a single server, this is a collection and not a shard
                projections: projections.clone(),
            };
            body_v = serde_json::to_vec::<DumpStartBodyWithProjections>(&body)
                .expect("could not serialize DumpStartBody");
        } else {
            let body = DumpStartBody {
                batch_size: load_config.batch_size,
                prefetch_count: load_config.prefetch_count,
                parallelism: load_config.parallelism,
                shards: shard_list.clone(), // in case of a single server, this is a collection and not a shard
            };
            body_v = serde_json::to_vec::<DumpStartBody>(&body)
                .expect("could not serialize DumpStartBody");
        }

        let resp = handle_auth(client.post(url), db_config)
            .body(body_v)
            .send()
            .await;
        let r = handle_arangodb_response(resp, |c| {
            c == StatusCode::NO_CONTENT || c == StatusCode::OK || c == StatusCode::CREATED
        })
        .await;
        if let Err(rr) = r {
            error = rr;
            error_happened = true;
            break;
        }
        let r = r.unwrap();
        let headers = r.headers();
        if let Some(id) = headers.get("X-Arango-Dump-Id") {
            if let Ok(id) = id.to_str() {
                dbservers.push(DBServerInfo {
                    dbserver: server.clone(),
                    dump_id: id.to_owned(),
                });
            }
        }
        debug!("Started dbserver {}", server);
    }

    let client_clone_for_cleanup = client.clone();
    let cleanup = |dbservers: Vec<DBServerInfo>| async move {
        debug!("Doing cleanup...");
        for dbserver in dbservers.iter() {
            let url = if deployment_type == DeploymentType::Cluster {
                make_url(
                    db_config,
                    &format!(
                        "/_api/dump/{}?dbserver={}",
                        dbserver.dump_id, dbserver.dbserver
                    ),
                )
            } else {
                make_url(db_config, &format!("/_api/dump/{}", dbserver.dump_id))
            };
            let resp = handle_auth(client_clone_for_cleanup.delete(url), db_config)
                .send()
                .await;
            let r =
                handle_arangodb_response(resp, |c| c == StatusCode::OK || c == StatusCode::CREATED)
                    .await;
            if let Err(rr) = r {
                eprintln!(
                    "An error in cancelling a dump context occurred, dbserver: {}, error: {}",
                    dbserver.dbserver, rr
                );
                // Otherwise ignore the error, this is just a cleanup!
            }
        }
    };

    if error_happened {
        // We need to cancel all dump contexts which we did get successfully:
        cleanup(dbservers).await;
        return Err(errors::GraphLoaderError::Other(error));
    }

    // We want to start the same number of tasks for each dbserver, each of
    // them will send next requests until no more data arrives

    #[derive(Debug)]
    struct TaskInfo {
        dbserver: DBServerInfo,
        current_batch_id: u64,
        last_batch_id: Option<u64>,
        id: u64,
    }

    if dbservers.is_empty() {
        // This actually happened writing integration tests, we cannot divide by zero
        error!("No dbserver found. List is empty.");
        return Err(GraphLoaderError::NoDatabaseServers);
    }

    let par_per_dbserver =
        (load_config.parallelism as usize + dbservers.len() - 1) / dbservers.len();
    let mut task_set = JoinSet::new();
    let mut endpoints_round_robin: usize = 0;
    let mut consumers_round_robin: usize = 0;
    for i in 0..par_per_dbserver {
        for dbserver in &dbservers {
            let mut task_info = TaskInfo {
                dbserver: dbserver.clone(),
                current_batch_id: i as u64,
                last_batch_id: None,
                id: i as u64,
            };
            //let client_clone = client.clone(); // the clones will share
            //                                   // the connection pool
            let use_tls = db_config.endpoints[0].starts_with("https://");
            let client_config = ClientConfig::builder()
                .n_retries(5)
                .use_tls(use_tls)
                .tls_cert_opt(db_config.tls_cert.clone())
                .build();
            let client_clone = build_client(&client_config)?;
            let endpoint_clone = db_config.endpoints[endpoints_round_robin].clone();
            endpoints_round_robin += 1;
            if endpoints_round_robin >= db_config.endpoints.len() {
                endpoints_round_robin = 0;
            }
            let database_clone = db_config.database.clone();
            let result_channel_clone = result_channels[consumers_round_robin].clone();
            consumers_round_robin += 1;
            if consumers_round_robin >= result_channels.len() {
                consumers_round_robin = 0;
            }
            let db_config_clone = db_config.clone();
            task_set.spawn(async move {
                loop {
                    let mut url = format!(
                        "{}/_db/{}/_api/dump/next/{}?dbserver={}&batchId={}",
                        endpoint_clone,
                        database_clone,
                        task_info.dbserver.dump_id,
                        task_info.dbserver.dbserver,
                        task_info.current_batch_id
                    );
                    if let Some(last) = task_info.last_batch_id {
                        url.push_str(&format!("&lastBatch={}", last));
                    }
                    let start = SystemTime::now();
                    debug!(
                        "{:?} Sending post request... {} {} {}",
                        start,
                        task_info.id,
                        task_info.dbserver.dbserver,
                        task_info.current_batch_id
                    );
                    let resp = handle_auth(client_clone.post(url), &db_config_clone)
                        .send()
                        .await;
                    let resp = handle_arangodb_response(resp, |c| {
                        c == StatusCode::OK || c == StatusCode::NO_CONTENT
                    })
                    .await?;
                    let end = SystemTime::now();
                    let dur = end.duration_since(start).unwrap();
                    if resp.status() == StatusCode::NO_CONTENT {
                        // Done, cleanup will be done later
                        debug!(
                            "{:?} Received final post response... {} {} {} {:?}",
                            end,
                            task_info.id,
                            task_info.dbserver.dbserver,
                            task_info.current_batch_id,
                            dur
                        );
                        return Ok::<(), String>(());
                    }
                    // Now the result was OK and the body is JSONL
                    task_info.last_batch_id = Some(task_info.current_batch_id);
                    task_info.current_batch_id += par_per_dbserver as u64;
                    let body = resp
                        .bytes()
                        .await
                        .map_err(|e| format!("Error in body: {:?}", e))?;
                    result_channel_clone
                        .send(body)
                        .await
                        .expect("Could not send to channel!");
                }
            });
        }
    }
    while let Some(res) = task_set.join_next().await {
        let r = res.unwrap();
        match r {
            Ok(_x) => {
                debug!("Got OK result!");
            }
            Err(msg) => {
                debug!("Got error result: {}", msg);
            }
        }
    }
    cleanup(dbservers).await;
    debug!("Done cleanup and channel is closed!");
    Ok(())
    // We drop the result_channel when we leave the function.
}

pub(crate) fn compute_faked_shard_map(coll_list: &[String]) -> ShardMap {
    // not really faked, but to be able to implement this quickly, we need to
    // simply build a map exposing the collection names instead of shard names.
    let mut result: ShardMap = HashMap::new();
    for c in coll_list.iter() {
        result.insert(c.clone().to_string(), vec![c.clone()]);
    }
    result
}

pub(crate) fn compute_shard_map(
    sd: &ShardDistribution,
    coll_list: &[String],
) -> Result<ShardMap, String> {
    let mut result: ShardMap = HashMap::new();
    for c in coll_list.iter() {
        // Handle the case of a smart edge collection. If c is
        // one, then we also find a collection called `_to_`+c.
        // In this case, we must not get those shards, because their
        // data is already contained in `_from_`+c, just sharded
        // differently.
        let mut ignore: HashSet<String> = HashSet::new();
        let smart_name = "_to_".to_owned() + c;
        match sd.results.get(&smart_name) {
            None => (),
            Some(coll_dist) => {
                // Keys of coll_dist are the shards, value has leader:
                for shard in coll_dist.plan.keys() {
                    ignore.insert(shard.clone());
                }
            }
        }
        match sd.results.get(c) {
            None => {
                return Err(format!("collection {} not found in shard distribution", c));
            }
            Some(coll_dist) => {
                // Keys of coll_dist are the shards, value has leader:
                for (shard, location) in &(coll_dist.plan) {
                    if !ignore.contains(shard) {
                        let leader = &(location.leader);
                        match result.get_mut(leader) {
                            None => {
                                result.insert(leader.clone(), vec![shard.clone()]);
                            }
                            Some(list) => {
                                list.push(shard.clone());
                            }
                        }
                    }
                }
            }
        }
    }
    Ok(result)
}
