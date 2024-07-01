use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::SystemTime;
use bytes::Bytes;
use log::{debug, error, info};
use reqwest::StatusCode;
use serde_json::Value;
use crate::{DatabaseConfiguration, DataLoadConfiguration};
use crate::client::{build_client, make_url};
use crate::errors::{GraphLoaderError};
use crate::request::handle_arangodb_response_with_parsed_body;
use crate::sharding::compute_shard_map;

// Define the necessary structs
pub struct CollectionInfo {
    pub name: String,
    pub fields: Vec<String>,
}

pub struct GraphLoader {
    db_config: DatabaseConfiguration,
    load_config: DataLoadConfiguration,
    graph_name: Option<String>,
    v_collections: HashMap<String, CollectionInfo>,
    e_collections: HashMap<String, CollectionInfo>,
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

        let mut graph_loader = GraphLoader { db_config, load_config, graph_name: None, v_collections, e_collections };
        graph_loader.initialize().await;
        Ok(graph_loader)
    }

    async fn initialize(&mut self) -> Result<(), GraphLoaderError> {
        let client = build_client(&self.db_config)?;

        // First ask for the shard distribution:
        let url = make_url(&self.db_config, "/_admin/cluster/shardDistribution");
        let resp = client.get(url).bearer_auth(&self.db_config.jwt_token).send().await;
        let shard_dist =
            handle_arangodb_response_with_parsed_body::<crate::sharding::ShardDistribution>(resp, StatusCode::OK)
                .await?;

        // Compute which shard we must get from which dbserver, we do vertices
        // and edges right away to be able to error out early:
        let vertex_map = compute_shard_map(&shard_dist, &self.get_vertex_collections_as_list())?;
        let edge_map = compute_shard_map(&shard_dist, &self.get_edge_collections_as_list())?;

        if vertex_map.is_empty() {
            error!("No vertex shards found!");
            return Err(GraphLoaderError::from("No vertex shards found!".to_string()));
        }
        if edge_map.is_empty() {
            error!("No edge shards found!");
            return Err(GraphLoaderError::from("No edge shards found!".to_string()));
        }
        info!(
          "{:?} Need to fetch data from {} vertex shards and {} edge shards...",
          std::time::SystemTime::now(),
          vertex_map.values().map(|v| v.len()).sum::<usize>(),
          edge_map.values().map(|v| v.len()).sum::<usize>()
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
            Err(_) => {
                return Err(GraphLoaderError::EmptyCollections);
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

    pub fn do_vertices<F>(&self, vertex_function: F)
    where
        F: Fn(Value) -> Result<(), Box<dyn std::error::Error>>,
    {
        {
            // We use multiple threads to receive the data in batches:
            let mut senders: Vec<tokio::sync::mpsc::Sender<Bytes>> = vec![];
            let mut consumers: Vec<JoinHandle<Result<(), String>>> = vec![];
            let prog_reported = Arc::new(Mutex::new(0_u64));
            for _i in 0..self.load_config.parallelism {
                let (sender, mut receiver) = tokio::sync::mpsc::channel::<Bytes>(10);
                senders.push(sender);
                let graph_clone = graph_arc.clone();
                let prog_reported_clone = prog_reported.clone();
                let fields = self.get_all_vertices_fields_as_list();
                let consumer = std::thread::spawn(move || -> Result<(), String> {
                    let begin = std::time::SystemTime::now();
                    while let Some(resp) = receiver.blocking_recv() {
                        let body = std::str::from_utf8(resp.as_ref())
                            .map_err(|e| format!("UTF8 error when parsing body: {:?}", e))?;
                        debug!(
                        "{:?} Received post response, body size: {}",
                        std::time::SystemTime::now().duration_since(begin),
                        body.len()
                    );
                        let mut vertex_keys: Vec<Vec<u8>> = Vec::with_capacity(400000);
                        let mut vertex_json: Vec<Vec<Value>> = vec![];
                        for line in body.lines() {
                            let v: Value = match serde_json::from_str(line) {
                                Err(err) => {
                                    return Err(format!(
                                        "Error parsing document for line:\n{}\n{:?}",
                                        line, err
                                    ));
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
                                    return Err(format!(
                                        "JSON is no object with a string _id attribute:\n{}",
                                        line
                                    ));
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
                        let nr_vertices: u64;
                        {
                            let mut graph = graph_clone.write().unwrap();
                            for i in 0..vertex_keys.len() {
                                let k = &vertex_keys[i];
                                let mut cols: Vec<Value> = vec![];
                                std::mem::swap(&mut cols, &mut vertex_json[i]);
                                graph.insert_vertex(k.clone(), cols);
                            }
                            nr_vertices = graph.number_of_vertices();
                        }
                        let mut prog = prog_reported_clone.lock().unwrap();
                        if nr_vertices > *prog + 1000000_u64 {
                            *prog = nr_vertices;
                            info!(
                            "{:?} Have imported {} vertices.",
                            std::time::SystemTime::now().duration_since(begin).unwrap(),
                            *prog
                        );
                        }
                    }
                    Ok(())
                });
                consumers.push(consumer);
            }
            get_all_shard_data(&req, &endpoints, &jwt_token, &ca_cert, &vertex_map, senders).await?;
            info!(
            "{:?} Got all data, processing...",
            std::time::SystemTime::now().duration_since(begin).unwrap()
        );
            for c in consumers {
                let _guck = c.join();
            }
            let mut graph = graph_arc.write().unwrap();
            graph.seal_vertices();
        }


        vertex_function(data).unwrap();
    }

    pub fn do_edges<F>(&self, edge_function: F)
    where
        F: Fn(Value) -> Result<(), Box<dyn std::error::Error>>,
    {
        // Placeholder: Replace with actual edge fetching logic
        for collection_info in self.e_collections.values() {
            let data = serde_json::json!({ "example": "edge_data" }); // Example data
            edge_function(data).unwrap();
        }
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

    let client = build_client(&db_config)?;
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