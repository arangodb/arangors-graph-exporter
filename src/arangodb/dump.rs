use crate::arangodb::info::DeploymentType;
use crate::client::auth::handle_auth;
use crate::client::config::ClientConfig;
use crate::input::load_request::{DataLoadRequest, DatabaseConfiguration};
use crate::{arangodb, client};
use bytes::Bytes;
use log::debug;
use reqwest::StatusCode;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::time::SystemTime;
use tokio::task::JoinSet;

#[derive(Debug, Serialize, Deserialize)]
pub struct ShardLocation {
    leader: String,
    followers: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct CollectionDistribution {
    plan: HashMap<String, ShardLocation>,
    current: HashMap<String, ShardLocation>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ShardDistribution {
    error: bool,
    code: i32,
    results: HashMap<String, CollectionDistribution>,
}

pub type ShardMap = HashMap<String, Vec<String>>;

pub fn compute_shard_map(
    sd_opt: &Option<ShardDistribution>,
    coll_list: &[String],
    deployment_type: &DeploymentType,
    endpoints: &[String],
) -> Result<ShardMap, String> {
    match deployment_type {
        DeploymentType::Single => {
            let mut result: ShardMap = HashMap::new();
            result.insert(endpoints[0].clone(), coll_list.to_vec());
            Ok(result)
        }
        DeploymentType::Cluster => {
            let mut result: ShardMap = HashMap::new();
            let sd = sd_opt
                .as_ref()
                .ok_or("Could not retrieve ShardDistribution".to_string())?;
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
                        for shard in (coll_dist.plan).keys() {
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
                            if ignore.get(shard).is_none() {
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
    }
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
struct DumpStartBody {
    batch_size: u64,
    prefetch_count: u32,
    parallelism: u32,
    shards: Vec<String>,
}

pub async fn get_all_shard_data(
    req: &DataLoadRequest,
    connection_config: &DatabaseConfiguration,
    shard_map: &ShardMap,
    result_channels: Vec<std::sync::mpsc::Sender<Bytes>>,
) -> Result<(), String> {
    let begin = SystemTime::now();

    let use_tls = connection_config.endpoints[0].starts_with("https://");
    let client_config = ClientConfig::builder()
        .n_retries(5)
        .use_tls(use_tls)
        .tls_cert_opt(connection_config.tls_cert.clone())
        .build();
    let client = client::build_client(&client_config)?;

    let make_url = |path: &str| -> String {
        connection_config.endpoints[0].clone() + "/_db/" + &req.database + path
    };

    // Start a single dump context on all involved dbservers, we can do
    // this sequentially, since it is not performance critical, we can
    // also use the same HTTP client and the same first endpoint:
    let mut dbservers: Vec<DBServerInfo> = vec![];
    let mut error_happened = false;
    let mut error: String = "".into();
    for (server, shard_list) in shard_map.iter() {
        let url = make_url(&format!("/_api/dump/start?dbserver={}", server));
        let body = DumpStartBody {
            batch_size: req.configuration.batch_size.unwrap(),
            prefetch_count: 5,
            parallelism: req.configuration.parallelism.unwrap(),
            shards: shard_list.clone(),
        };
        let body_v =
            serde_json::to_vec::<DumpStartBody>(&body).expect("could not serialize DumpStartBody");
        let resp = handle_auth(client.post(url), connection_config)
            .body(body_v)
            .send()
            .await;
        let r = arangodb::handle_arangodb_response(resp, |c| {
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
            let url = make_url(&format!(
                "/_api/dump/{}?dbserver={}",
                dbserver.dump_id, dbserver.dbserver
            ));
            let resp = handle_auth(client_clone_for_cleanup.delete(url), connection_config)
                .send()
                .await;
            let r = arangodb::handle_arangodb_response(resp, |c| {
                c == StatusCode::OK || c == StatusCode::CREATED
            })
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
        return Err(error);
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

    let par_per_dbserver = if dbservers.is_empty() {
        0
    } else {
        (req.configuration.parallelism.unwrap() as usize + dbservers.len() - 1) / dbservers.len()
    };

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
            let client_clone = client::build_client(&client_config)?;
            let endpoint_clone = connection_config.endpoints[endpoints_round_robin].clone();
            endpoints_round_robin += 1;
            if endpoints_round_robin >= connection_config.endpoints.len() {
                endpoints_round_robin = 0;
            }
            let database_clone = req.database.clone();
            let result_channel_clone = result_channels[consumers_round_robin].clone();
            consumers_round_robin += 1;
            if consumers_round_robin >= result_channels.len() {
                consumers_round_robin = 0;
            }
            let connection_config_clone = (*connection_config).clone();
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
                        start.duration_since(begin).unwrap(),
                        task_info.id,
                        task_info.dbserver.dbserver,
                        task_info.current_batch_id
                    );
                    let resp = handle_auth(client_clone.post(url), &connection_config_clone)
                        .send()
                        .await;
                    let resp = arangodb::handle_arangodb_response(resp, |c| {
                        c == StatusCode::OK || c == StatusCode::NO_CONTENT
                    })
                    .await?;
                    let end = SystemTime::now();
                    let dur = end.duration_since(start).unwrap();
                    if resp.status() == StatusCode::NO_CONTENT {
                        // Done, cleanup will be done later
                        debug!(
                            "{:?} Received final post response... {} {} {} {:?}",
                            end.duration_since(begin).unwrap(),
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

#[derive(Debug, Clone)]
struct DBServerInfo {
    dbserver: String,
    dump_id: String,
}
