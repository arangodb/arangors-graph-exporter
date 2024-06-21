use crate::arangodb::info::{DeploymentType, SupportInfo};
use crate::client::auth::handle_auth;
use crate::client::config::ClientConfig;
use crate::input::load_request::{DataLoadRequest, DatabaseConfiguration, CollectionName, CollectionAttribute, CollectionDescription, generate_collection_list};
use crate::{arangodb, client};
use bytes::Bytes;
use log::{debug, info};
use reqwest::StatusCode;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::ops::{Deref, DerefMut};
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

#[derive(Clone)]
pub struct CollectionInfo {
    pub description: CollectionDescription,
    pub endpoint: String,
    pub shards: Vec<Shard>,
}

impl CollectionInfo {
    pub fn new(description: CollectionDescription, endpoint: Endpoint, shards: Vec<Shard>) -> Self {
        Self {
            description,
            endpoint,
            shards,
        }
    }

    pub fn generate_shard_map(&self) -> ShardMap {
        let mut shard_map: HashMap<Endpoint, Vec<Shard>> = HashMap::new();
        shard_map.insert(self.endpoint.clone(), self.shards.clone());
        shard_map
    }

    pub fn set_shards(&mut self, shards: Vec<Shard>) {
        self.shards = shards;
    }
}

#[derive(Clone)]
pub struct CollectionsInfo(pub HashMap<CollectionName, CollectionInfo>);

impl CollectionsInfo {
    pub fn new() -> Self {
        CollectionsInfo(HashMap::new())
    }

    pub fn add_collection(&mut self, name: CollectionName, info: CollectionInfo) {
        self.0.insert(name, info);
    }

    pub fn get_collection_info(&self, name: &CollectionName) -> Option<&CollectionInfo> {
        self.0.get(name)
    }

    pub fn total_shard_amount(&self) -> usize {
        let mut total = 0;
        for (_, info) in self.0.iter() {
            total += info.shards.len();
        }
        total
    }

    pub fn no_shards_found(&self) -> bool {
        self.total_shard_amount() == 0
    }
}

impl Deref for CollectionsInfo {
    type Target = HashMap<CollectionName, CollectionInfo>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for CollectionsInfo {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

pub type Endpoint = String;
pub type Shard = String;
// this depends on whether we are in a cluster or not
// in cluster case it will be a shard, in single case it will be a collection

pub type ShardMap = HashMap<Endpoint, Vec<Shard>>;

pub fn compute_shard_infos(
    sd_opt: &Option<ShardDistribution>,
    collections: Vec<CollectionDescription>,
    deployment_type: &DeploymentType,
    endpoints: &[String],
) -> Result<CollectionsInfo, String> {
    match deployment_type {
        DeploymentType::Single => {
            let mut result = CollectionsInfo::new();
            let c_list = generate_collection_list(collections.clone());
            for c in collections.into_iter() {
                result.insert(c.name.clone(), CollectionInfo::new(c, endpoints[0].clone(), c_list.clone()));
            }
            Ok(result)
        }
        DeploymentType::Cluster => {
            let mut result = CollectionsInfo::new();
            let sd = sd_opt
                .as_ref()
                .ok_or("Could not retrieve ShardDistribution".to_string())?;
            for collection in collections.into_iter() {
                let c_name = &collection.name;
                // Handle the case of a smart edge collection. If c is
                // one, then we also find a collection called `_to_`+c.
                // In this case, we must not get those shards, because their
                // data is already contained in `_from_`+c, just sharded
                // differently.
                let mut ignore: HashSet<String> = HashSet::new();
                let smart_name = "_to_".to_owned() + c_name;
                match sd.results.get(&smart_name) {
                    None => (),
                    Some(coll_dist) => {
                        // Keys of coll_dist are the shards, value has leader:
                        for shard in (coll_dist.plan).keys() {
                            ignore.insert(shard.clone());
                        }
                    }
                }
                match sd.results.get(c_name) {
                    None => {
                        return Err(format!("collection {} not found in shard distribution", c_name));
                    }
                    Some(coll_dist) => {
                        // Keys of coll_dist are the shards, value has leader:
                        for (shard, location) in &(coll_dist.plan) {
                            if ignore.get(shard).is_none() {
                                let leader = &(location.leader);
                                match result.get_mut(leader) {
                                    None => {
                                        result.insert(c_name.clone(), CollectionInfo::new(collection.clone(), leader.clone(), vec![shard.clone()]));
                                    }
                                    Some(list) => {
                                        list.shards.push(shard.clone());
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
    shards: Vec<Shard>,
    projections: HashMap<CollectionName, Vec<CollectionAttribute>>,
}

pub fn generate_fields_map(collections_info: &CollectionsInfo) -> HashMap<CollectionName, Vec<CollectionAttribute>> {
    let mut fields_map: HashMap<CollectionName, Vec<CollectionAttribute>> = HashMap::new();
    for (collection_name, collection_info) in collections_info.iter() {
        fields_map.insert(collection_name.clone(), collection_info.description.fields.clone());
    }
    fields_map
}

pub fn generate_shard_map(collections_info: &CollectionsInfo) -> HashMap<Endpoint, Vec<Shard>> {
    let mut shard_map: HashMap<Endpoint, Vec<Shard>> = HashMap::new();

    for (_collection_name, collection_info) in collections_info.iter() {
        let endpoint = &collection_info.endpoint;
        let shards = &collection_info.shards;
        let entry = shard_map.entry(endpoint.clone()).or_insert_with(Vec::new);

        for shard in shards.iter() {
            if !entry.contains(shard) {
                entry.push(shard.clone());
            }
            entry.push(shard.clone());
        }
    }

    shard_map
}

pub async fn get_all_shard_data(
    req: &DataLoadRequest,
    connection_config: &DatabaseConfiguration,
    support_info: &SupportInfo,
    collections_info: CollectionsInfo,
    result_channels: Vec<std::sync::mpsc::Sender<Bytes>>,
    is_edge: bool,
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
    let shard_map = generate_shard_map(&collections_info);

    let collection_type = if is_edge { "edge" } else { "vertex" };
    info!(
        "{:?} Need to fetch data from {} {} shards...",
        std::time::SystemTime::now().duration_since(begin).unwrap(),
        shard_map.values().map(|v| v.len()).sum::<usize>(), collection_type
    );

    for (endpoint, shard_list) in shard_map.iter() {
        let url = if support_info.deployment == DeploymentType::Cluster {
            make_url(&format!("/_api/dump/start?dbserver={}", endpoint))
        } else {
            make_url("/_api/dump/start")
        };

        let mut local_projections = HashMap::new();
        if is_edge {
            // those values are always present and required in an edge collection
            local_projections.insert("_from".to_string(), vec!["_from".to_string()]);
            local_projections.insert("_to".to_string(), vec!["_to".to_string()]);
            // additional properties currently not supported - need to be implemented
        } else {
            // this value is always present and required in a vertex collection
            local_projections.insert("_id".to_string(), vec!["_id".to_string()]);
            let vcf_map = generate_fields_map(&collections_info);

            for (collection_name, fields) in vcf_map.iter() {
                local_projections.insert(collection_name.clone(), fields.clone());
            }
        }

        let body = DumpStartBody {
            batch_size: req.configuration.batch_size.unwrap(),
            prefetch_count: 5,
            parallelism: req.configuration.parallelism.unwrap(),
            shards: shard_list.clone(),
            projections: local_projections.clone(),
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
                    dbserver: endpoint.clone(),
                    dump_id: id.to_owned(),
                });
            }
        }
        debug!("Started dbserver {}", endpoint);
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
