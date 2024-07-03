use std::collections::HashMap;
use std::thread::JoinHandle;
use std::time::SystemTime;
use bytes::Bytes;
use log::{debug, error, info};
use reqwest::StatusCode;
use serde_json::Value;
use crate::{client, DatabaseConfiguration, DataLoadConfiguration};
use crate::client::{build_client, make_url};
use crate::client::config::ClientConfig;
use crate::errors::{GraphLoaderError};
use crate::request::handle_arangodb_response_with_parsed_body;
use crate::sharding::compute_shard_map;

// Define the necessary structs
#[derive(Clone)]
pub struct CollectionInfo {
    pub name: String,
    pub fields: Vec<String>,
}

pub struct GraphLoader {
    db_config: DatabaseConfiguration,
    load_config: DataLoadConfiguration,
    v_collections: HashMap<String, CollectionInfo>,
    e_collections: HashMap<String, CollectionInfo>,
    vertex_map: crate::sharding::ShardMap,
    edge_map: crate::sharding::ShardMap,
}

fn collection_name_from_id(id: &str) -> String {
    let pos = id.find('/');
    match pos {
        None => "".to_string(),
        Some(p) => id[0..p].to_string(),
    }
}

impl GraphLoader {
    pub async fn new(
        db_config: DatabaseConfiguration,
        load_config: DataLoadConfiguration,
        vertex_collections: Vec<CollectionInfo>,
        edge_collections: Vec<CollectionInfo>,
    ) -> Result<GraphLoader, GraphLoaderError> {
        if vertex_collections.is_empty() && edge_collections.is_empty() {
            return Err(GraphLoaderError::EmptyCollections);
        }

        let v_collections = vertex_collections.into_iter().map(|c| (c.name.clone(), c)).collect();
        let e_collections = edge_collections.into_iter().map(|c| (c.name.clone(), c)).collect();

        let mut graph_loader = GraphLoader { db_config, load_config, v_collections, e_collections, vertex_map: HashMap::new(), edge_map: HashMap::new() };
        let init_result = graph_loader.initialize().await;
        if let Err(e) = init_result {
            return Err(e);
        }

        Ok(graph_loader)
    }

    async fn initialize(&mut self) -> Result<(), GraphLoaderError> {
        let use_tls = self.db_config.endpoints[0].starts_with("https://");
        let client_config = ClientConfig::builder()
            .n_retries(5)
            .use_tls(use_tls)
            .tls_cert_opt(self.db_config.tls_cert.clone())
            .build();
        let client = client::build_client(&client_config)?;

        // First ask for the shard distribution:
        let url = make_url(&self.db_config, "/_admin/cluster/shardDistribution");
        let resp = client.get(url).bearer_auth(&self.db_config.jwt_token).send().await;
        let shard_dist =
            handle_arangodb_response_with_parsed_body::<crate::sharding::ShardDistribution>(resp, StatusCode::OK)
                .await?;

        // Compute which shard we must get from which dbserver, we do vertices
        // and edges right away to be able to error out early:
        self.vertex_map = compute_shard_map(&shard_dist, &self.get_vertex_collections_as_list())?;
        self.edge_map = compute_shard_map(&shard_dist, &self.get_edge_collections_as_list())?;

        if self.vertex_map.is_empty() {
            error!("No vertex shards found!");
            return Err(GraphLoaderError::from("No vertex shards found!".to_string()));
        }
        if self.edge_map.is_empty() {
            error!("No edge shards found!");
            return Err(GraphLoaderError::from("No edge shards found!".to_string()));
        }
        info!(
          "{:?} Need to fetch data from {} vertex shards and {} edge shards...",
          std::time::SystemTime::now(),
          self.vertex_map.values().map(|v| v.len()).sum::<usize>(),
          self.edge_map.values().map(|v| v.len()).sum::<usize>()
        );

        Ok(())
    }

    pub async fn new_named(
        db_config: DatabaseConfiguration,
        load_config: DataLoadConfiguration,
        graph_name: String,
    ) -> Result<GraphLoader, GraphLoaderError> {
        let vertex_coll_list;
        let edge_coll_list;

        match get_graph_collections(&db_config, graph_name).await {
            Ok((vertex_collections, edge_collections)) => {
                vertex_coll_list = vertex_collections
                    .iter()
                    .map(|c| CollectionInfo {
                        name: c.clone(),
                        fields: vec![],
                    })
                    .collect();

                edge_coll_list = edge_collections
                    .iter()
                    .map(|c| CollectionInfo {
                        name: c.clone(),
                        fields: vec![],
                    })
                    .collect();
            }
            Err(err) => {
                return Err(err);
            }
        }

        let graph_loader = GraphLoader::new(db_config, load_config, vertex_coll_list, edge_coll_list).await?;
        Ok(graph_loader)
    }

    pub async fn new_custom(
        db_config: DatabaseConfiguration,
        load_config: DataLoadConfiguration,
        vertex_collections: Vec<CollectionInfo>,
        edge_collections: Vec<CollectionInfo>,
    ) -> Result<Self, GraphLoaderError> {
        if vertex_collections.is_empty() || edge_collections.is_empty() {
            return Err(GraphLoaderError::EmptyCollections);
        }
        let graph_loader = GraphLoader::new(db_config, load_config, vertex_collections, edge_collections).await?;
        Ok(graph_loader)
    }

    pub async fn do_vertices<F>(&self, vertices_function: F) -> Result<(), GraphLoaderError>
    where
        F: Fn(&Vec<Vec<u8>>, &mut Vec<Vec<Value>>) -> Result<(), GraphLoaderError> + Send + Sync + Clone + 'static,
    {
        {
            // We use multiple threads to receive the data in batches:
            let mut senders: Vec<tokio::sync::mpsc::Sender<Bytes>> = vec![];
            let mut consumers: Vec<JoinHandle<Result<(), GraphLoaderError>>> = vec![];

            for _i in 0..self.load_config.parallelism {
                let (sender, mut receiver) = tokio::sync::mpsc::channel::<Bytes>(10);
                senders.push(sender);

                let fields = self.get_all_vertices_fields_as_list();
                let closure_clone = vertices_function.clone();
                let consumer = std::thread::spawn(move || -> Result<(), GraphLoaderError> {
                    let begin = SystemTime::now();
                    while let Some(resp) = receiver.blocking_recv() {
                        let body_result = std::str::from_utf8(resp.as_ref());
                        let body = match body_result {
                            Ok(body) => body,
                            Err(e) => return Err(GraphLoaderError::Utf8Error(format!("UTF8 error when parsing body: {:?}", e))),
                        };
                        debug!(
                        "{:?} Received post response, body size: {}",
                        SystemTime::now().duration_since(begin),
                        body.len()
                    );
                        let mut vertex_keys: Vec<Vec<u8>> = Vec::with_capacity(400000);
                        let mut vertex_json: Vec<Vec<Value>> = vec![];
                        for line in body.lines() {
                            let v: Value = match serde_json::from_str(line) {
                                Err(err) => {
                                    return Err(GraphLoaderError::JsonParseError(format!(
                                        "Error parsing document for line:\n{}\n{:?}", line, err
                                    )));
                                }
                                Ok(val) => val,
                            };
                            let id = &v["_id"];
                            let idstr: &String = match id {
                                Value::String(i) => {
                                    let mut buf = vec![];
                                    buf.extend_from_slice(i[..].as_bytes());
                                    vertex_keys.push(buf);
                                    i
                                }
                                _ => {
                                    return Err(GraphLoaderError::JsonParseError(format!(
                                        "JSON is no object with a string _id attribute:\n{}",
                                        line
                                    )));
                                }
                            };
                            // If we get here, we have to extract the field
                            // values in `fields` from the json and store it
                            // to vertex_json:
                            let get_value = |v: &Value, field: &str| -> Value {
                                if field == "@collection_name" {
                                    Value::String(collection_name_from_id(idstr))
                                } else {
                                    v[field].clone()
                                }
                            };

                            let mut cols: Vec<Value> = Vec::with_capacity(fields.len());
                            for f in fields.iter() {
                                let j = get_value(&v, f);
                                cols.push(j);
                            }
                            vertex_json.push(cols);
                        }
                        closure_clone(&vertex_keys, &mut vertex_json)?;
                    }
                    Ok(())
                });
                consumers.push(consumer);
            }
            let shard_result = crate::sharding::get_all_shard_data(&self.db_config, &self.load_config, &self.vertex_map, senders)
                .await;
            match shard_result {
                Err(e) => {
                    error!("Error fetching vertex data: {:?}", e);
                    return Err(GraphLoaderError::from(format!("Failed to get shard data: {}", e)));
                }
                Ok(_) => {}
            }

            info!(
            "{:?} Got all data, processing...",
            SystemTime::now()
        );
            for c in consumers {
                let _guck = c.join();
            }
        }
        Ok(())
    }

    pub async fn do_edges<F>(&self, edges_function: F) -> Result<(), GraphLoaderError>
    where
        F: Fn(&Vec<Vec<u8>>, &Vec<Vec<u8>>) -> Result<(), GraphLoaderError> + Send + Sync + Clone + 'static,
    {
        let mut senders: Vec<tokio::sync::mpsc::Sender<Bytes>> = vec![];
        let mut consumers: Vec<JoinHandle<Result<(), GraphLoaderError>>> = vec![];

        for _i in 0..self.load_config.parallelism {
            let (sender, mut receiver) = tokio::sync::mpsc::channel::<Bytes>(10);
            senders.push(sender);
            let closure_clone = edges_function.clone();
            let consumer = std::thread::spawn(move || -> Result<(), GraphLoaderError> {
                while let Some(resp) = receiver.blocking_recv() {
                    let body = std::str::from_utf8(resp.as_ref())
                        .map_err(|e| format!("UTF8 error when parsing body: {:?}", e))?;
                    let mut froms: Vec<Vec<u8>> = Vec::with_capacity(1000000);
                    let mut tos: Vec<Vec<u8>> = Vec::with_capacity(1000000);
                    for line in body.lines() {
                        let v: Value = match serde_json::from_str(line) {
                            Err(err) => {
                                return Err(GraphLoaderError::from(format!(
                                    "Error parsing document for line:\n{}\n{:?}",
                                    line, err
                                )));
                            }
                            Ok(val) => val,
                        };
                        let from = &v["_from"];
                        match from {
                            Value::String(i) => {
                                let mut buf = vec![];
                                buf.extend_from_slice(i[..].as_bytes());
                                froms.push(buf);
                            }
                            _ => {
                                return Err(GraphLoaderError::from(format!(
                                    "JSON is no object with a string _from attribute:\n{}",
                                    line
                                )));
                            }
                        }
                        let to = &v["_to"];
                        match to {
                            Value::String(i) => {
                                let mut buf = vec![];
                                buf.extend_from_slice(i[..].as_bytes());
                                tos.push(buf);
                            }
                            _ => {
                                return Err(GraphLoaderError::from(format!(
                                    "JSON is no object with a string _from attribute:\n{}",
                                    line
                                )));
                            }
                        }
                    }

                    closure_clone(&froms, &tos)?;
                }
                Ok(())
            });
            consumers.push(consumer);
        }

        let shard_result = crate::sharding::get_all_shard_data(&self.db_config, &self.load_config, &self.edge_map, senders)
            .await;
        match shard_result {
            Err(e) => {
                error!("Error fetching vertex data: {:?}", e);
                return Err(e);
            }
            Ok(_) => {}
        }

        info!(
            "{:?} Got all edge data, processing...",
            std::time::SystemTime::now()
        );
        for c in consumers {
            let _guck = c.join();
        }
        Ok(())
    }

    pub fn get_vertex_collections_as_list(&self) -> Vec<String> {
        self.v_collections.keys().cloned().collect()
    }

    pub fn get_edge_collections_as_list(&self) -> Vec<String> {
        self.e_collections.keys().cloned().collect()
    }

    pub fn get_all_vertices_fields_as_list(&self) -> Vec<String> {
        self.v_collections.values().flat_map(|c| c.fields.clone()).collect()
    }
}

async fn get_graph_collections(
    db_config: &DatabaseConfiguration, graph_name: String,
) -> Result<(Vec<String>, Vec<String>), GraphLoaderError> {
    let param_url = format!("/_api/gharial/{}", graph_name);
    let url = make_url(&db_config, &param_url);
    let graph_name = graph_name.clone();
    let (vertex_collections, edge_collections) =
        fetch_edge_and_vertex_collections_by_graph(db_config, url).await?;
    info!(
            "{:?} Got vertex collections: {:?}, edge collections: {:?} from graph definition for: {:?}.",
            SystemTime::now(),
            vertex_collections, edge_collections, graph_name
        );

    Ok((vertex_collections, edge_collections))
}

async fn fetch_edge_and_vertex_collections_by_graph(db_config: &DatabaseConfiguration,
                                                    url: String,
) -> Result<(Vec<String>, Vec<String>), GraphLoaderError> {
    let mut edge_collection_names = vec![];
    let mut vertex_collection_names = vec![];

    let use_tls = db_config.endpoints[0].starts_with("https://");
    let client_config = ClientConfig::builder()
        .n_retries(5)
        .use_tls(use_tls)
        .tls_cert_opt(db_config.tls_cert.clone())
        .build();
    let client = build_client(&client_config)?;
    let jwt_token = &db_config.jwt_token;
    let resp = client.get(url).bearer_auth(jwt_token).send().await;
    let parsed_response =
        handle_arangodb_response_with_parsed_body::<serde_json::Value>(resp, StatusCode::OK)
            .await?;
    let graph = parsed_response["graph"]
        .as_object()
        .ok_or(GraphLoaderError::GraphNotObject)?;
    let edge_definitions = graph
        .get("edgeDefinitions")
        .ok_or(GraphLoaderError::NoEdgeDefinitions)?
        .as_array()
        .ok_or(GraphLoaderError::EdgeDefinitionsNotArray)?;

    let mut non_unique_vertex_collection_names = vec![];
    for edge_definition in edge_definitions {
        let edge_collection_name = edge_definition["collection"]
            .as_str()
            .ok_or(GraphLoaderError::CollectionNotString)?;
        edge_collection_names.push(edge_collection_name.to_string());

        let from = edge_definition["from"]
            .as_array()
            .ok_or(GraphLoaderError::FromNotArray)?;
        for vertex in from {
            let vertex_collection_name =
                vertex.as_str().ok_or(GraphLoaderError::FromCollectionNotString)?;
            non_unique_vertex_collection_names.push(vertex_collection_name.to_string());
        }

        let to = edge_definition["to"]
            .as_array()
            .ok_or(GraphLoaderError::ToNotArray)?;
        for vertex in to {
            let vertex_collection_name = vertex.as_str().ok_or(GraphLoaderError::ToCollectionNotString)?;
            non_unique_vertex_collection_names.push(vertex_collection_name.to_string());
        }
    }

    non_unique_vertex_collection_names.sort();
    non_unique_vertex_collection_names.dedup();
    vertex_collection_names.append(&mut non_unique_vertex_collection_names);

    Ok((vertex_collection_names, edge_collection_names))
}