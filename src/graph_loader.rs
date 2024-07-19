use crate::aql::get_all_data_aql;
use crate::client::auth::handle_auth;
use crate::client::config::ClientConfig;
use crate::client::{build_client, make_url};
use crate::errors::GraphLoaderError;
use crate::request::handle_arangodb_response_with_parsed_body;
use crate::sharding::{compute_faked_shard_map, compute_shard_map};
use crate::types::info::{DeploymentType, LoadStrategy, SupportInfo, VersionInformation};
use crate::{DataLoadConfiguration, DatabaseConfiguration};
use bytes::Bytes;
use log::{debug, error, info};
use reqwest::StatusCode;
use reqwest_middleware::ClientWithMiddleware;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::cmp::PartialEq;
use std::collections::{HashMap, HashSet};
use std::num::ParseIntError;
use std::thread::JoinHandle;
use std::time::SystemTime;

static MIN_SUPPORTED_MINOR_VERSIONS: &[(u8, u8)] = &[(3, 12)];

// Define the necessary structs
#[derive(Clone)]
pub struct CollectionInfo {
    pub name: String,
    pub fields: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct CursorResult {
    result: Vec<Value>,
}

pub struct GraphLoader {
    db_config: DatabaseConfiguration,
    load_config: DataLoadConfiguration,
    v_collections: HashMap<String, CollectionInfo>,
    e_collections: HashMap<String, CollectionInfo>,
    vertex_map: crate::sharding::ShardMap,
    edge_map: crate::sharding::ShardMap,
    // should be used as private
    load_strategy: Option<LoadStrategy>,
    support_info: Option<SupportInfo>,
}

fn collection_name_from_id(id: &str) -> String {
    match id.find('/') {
        None => "".to_string(),
        Some(p) => id[0..p].to_string(),
    }
}

impl PartialEq<DeploymentType> for &DeploymentType {
    fn eq(&self, other: &DeploymentType) -> bool {
        matches!(
            (self, other),
            (DeploymentType::Cluster, DeploymentType::Cluster)
                | (DeploymentType::Single, DeploymentType::Single)
        )
    }
}

impl GraphLoader {
    pub async fn new(
        db_config: DatabaseConfiguration,
        load_config: DataLoadConfiguration,
        vertex_collections: Vec<CollectionInfo>,
        edge_collections: Vec<CollectionInfo>,
    ) -> Result<GraphLoader, GraphLoaderError> {
        let v_collections = vertex_collections
            .into_iter()
            .map(|c| (c.name.clone(), c))
            .collect();
        let e_collections = edge_collections
            .into_iter()
            .map(|c| (c.name.clone(), c))
            .collect();

        let mut graph_loader = GraphLoader {
            db_config,
            load_config,
            v_collections,
            e_collections,
            vertex_map: HashMap::new(),
            edge_map: HashMap::new(),
            load_strategy: None,
            support_info: None,
        };
        let init_result = graph_loader.initialize().await;
        init_result?;

        Ok(graph_loader)
    }

    async fn does_arangodb_supports_dump_endpoint(
        &self,
        client: &ClientWithMiddleware,
    ) -> Result<bool, String> {
        let server_version_url = self.db_config.endpoints[0].clone() + "/_api/version";
        let resp = handle_auth(client.get(server_version_url), &self.db_config)
            .send()
            .await;
        let version_info_result =
            handle_arangodb_response_with_parsed_body::<VersionInformation>(resp, StatusCode::OK)
                .await;
        if let Err(e) = version_info_result {
            return Err(e.to_string());
        }
        let version_info = version_info_result.unwrap();

        let version_parts: Vec<&str> = version_info.version.split('.').collect();
        if version_parts.len() < 3 {
            return Err(format!(
                "Unable to parse ArangoDB Version - got {}",
                version_info.version
            ));
        }

        let supports_dump_endpoint = {
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
        Ok(supports_dump_endpoint)
    }

    async fn get_arangodb_support_information(
        &self,
        client: &ClientWithMiddleware,
    ) -> Result<SupportInfo, String> {
        let server_information_url = self.db_config.endpoints[0].clone() + "/_admin/support-info";
        let support_info_res = handle_auth(client.get(server_information_url), &self.db_config)
            .send()
            .await;
        let support_info_result = handle_arangodb_response_with_parsed_body::<SupportInfo>(
            support_info_res,
            StatusCode::OK,
        )
        .await;
        if let Err(e) = support_info_result {
            return Err(e.to_string());
        }
        let support_info = support_info_result.unwrap();
        Ok(support_info)
    }

    fn identify_arangodb_load_strategy(&self, dump_support_enabled: bool) -> LoadStrategy {
        let support_info = &self
            .support_info
            .as_ref()
            .unwrap()
            .deployment
            .deployment_type;
        if !dump_support_enabled && support_info == DeploymentType::Single {
            LoadStrategy::Aql
        } else {
            LoadStrategy::Dump
        }
    }

    async fn identify_load_strategy(
        &mut self,
        client: &ClientWithMiddleware,
    ) -> Result<(), String> {
        let dump_support_enabled = self.does_arangodb_supports_dump_endpoint(client).await?;
        self.support_info = Some(self.get_arangodb_support_information(client).await?);
        self.load_strategy = Some(self.identify_arangodb_load_strategy(dump_support_enabled));
        Ok(())
    }

    fn verify_parameters(&self) -> Result<(), GraphLoaderError> {
        if !self.get_all_vertices_fields_as_list().is_empty()
            && self.load_config.load_all_vertex_attributes
        {
            return Err(GraphLoaderError::from(
                "load_all_vertex_attributes is set to true, but vertex collections are not empty."
                    .to_string(),
            ));
        }
        if !self.get_all_edges_fields_as_list().is_empty()
            && self.load_config.load_all_edge_attributes
        {
            return Err(GraphLoaderError::from(
                "load_all_edge_attributes is set to true, but edge collections are not empty."
                    .to_string(),
            ));
        }
        Ok(())
    }

    async fn initialize(&mut self) -> Result<(), GraphLoaderError> {
        self.verify_parameters()?;

        let use_tls = self.db_config.endpoints[0].starts_with("https://");
        let client_config = ClientConfig::builder()
            .n_retries(5)
            .use_tls(use_tls)
            .tls_cert_opt(self.db_config.tls_cert.clone())
            .build();
        let client = build_client(&client_config)?;
        self.identify_load_strategy(&client).await?;

        let load_strategy = self.load_strategy.as_ref().unwrap();
        let deployment_type = &self
            .support_info
            .as_ref()
            .unwrap()
            .deployment
            .deployment_type;

        if load_strategy == &LoadStrategy::Dump {
            if *deployment_type == DeploymentType::Cluster {
                // Compute which shard we must get from which dbserver, we do vertices
                // and edges right away to be able to error out early:

                // First ask for the shard distribution:
                let url = make_url(&self.db_config, "/_admin/cluster/shardDistribution");
                let resp = handle_auth(client.get(url), &self.db_config).send().await;

                let shard_dist = handle_arangodb_response_with_parsed_body::<
                    crate::sharding::ShardDistribution,
                >(resp, StatusCode::OK)
                .await?;
                info!("Using dump strategy for loading data.");
                self.vertex_map =
                    compute_shard_map(&shard_dist, &self.get_vertex_collections_as_list())?;
                self.edge_map =
                    compute_shard_map(&shard_dist, &self.get_edge_collections_as_list())?;
            } else {
                self.vertex_map = compute_faked_shard_map(&self.get_vertex_collections_as_list());
                self.edge_map = compute_faked_shard_map(&self.get_edge_collections_as_list());
            }
            info!(
                "{:?} Need to fetch data from {} vertex shards and {} edge shards...",
                std::time::SystemTime::now(),
                self.vertex_map.values().map(|v| v.len()).sum::<usize>(),
                self.edge_map.values().map(|v| v.len()).sum::<usize>()
            );
        } else {
            info!("Using AQL strategy for loading data.");
        }

        Ok(())
    }

    pub async fn new_named(
        db_config: DatabaseConfiguration,
        load_config: DataLoadConfiguration,
        graph_name: String,
        vertex_global_fields: Option<Vec<String>>,
    ) -> Result<GraphLoader, GraphLoaderError> {
        let vertex_coll_list;
        let edge_coll_list;

        match get_graph_collections(&db_config, graph_name).await {
            Ok((vertex_collections, edge_collections)) => {
                vertex_coll_list = vertex_collections
                    .iter()
                    .map(|c| CollectionInfo {
                        name: c.clone(),
                        fields: vertex_global_fields.clone().unwrap_or_default(),
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

        let graph_loader =
            GraphLoader::new(db_config, load_config, vertex_coll_list, edge_coll_list).await?;
        Ok(graph_loader)
    }

    pub async fn new_custom(
        db_config: DatabaseConfiguration,
        load_config: DataLoadConfiguration,
        vertex_collections: Vec<CollectionInfo>,
        edge_collections: Vec<CollectionInfo>,
    ) -> Result<Self, GraphLoaderError> {
        let graph_loader =
            GraphLoader::new(db_config, load_config, vertex_collections, edge_collections).await?;
        Ok(graph_loader)
    }

    pub async fn do_vertices<F>(&self, vertices_function: F) -> Result<(), GraphLoaderError>
    where
        F: Fn(&Vec<Vec<u8>>, &mut Vec<Vec<Value>>, &Vec<String>) -> Result<(), GraphLoaderError>
            + Send
            + Sync
            + Clone
            + 'static,
    {
        {
            // We use multiple threads to receive the data in batches:
            let mut senders: Vec<tokio::sync::mpsc::Sender<Bytes>> = vec![];
            let mut consumers: Vec<JoinHandle<Result<(), GraphLoaderError>>> = vec![];

            for _i in 0..self.load_config.parallelism {
                let (sender, mut receiver) = tokio::sync::mpsc::channel::<Bytes>(10);
                senders.push(sender);

                let vertex_global_fields = self.get_all_vertices_fields_as_list();
                let insert_vertex_clone = vertices_function.clone();
                let strategy_clone = self.load_strategy;
                let load_config_clone = self.load_config.clone();

                let consumer = std::thread::spawn(move || -> Result<(), GraphLoaderError> {
                    let begin = SystemTime::now();
                    while let Some(resp) = receiver.blocking_recv() {
                        let body_result = std::str::from_utf8(resp.as_ref());
                        let body = match body_result {
                            Ok(body) => body,
                            Err(e) => {
                                return Err(GraphLoaderError::Utf8Error(format!(
                                    "UTF8 error when parsing body: {:?}",
                                    e
                                )))
                            }
                        };
                        debug!(
                            "{:?} Received post response, body size: {}",
                            SystemTime::now().duration_since(begin),
                            body.len()
                        );
                        let mut vertex_ids: Vec<Vec<u8>> = Vec::with_capacity(400000);
                        let mut vertex_json: Vec<Vec<Value>> = Vec::with_capacity(400000);

                        if strategy_clone == Option::from(LoadStrategy::Dump) {
                            for line in body.lines() {
                                let mut vertex: Value = match serde_json::from_str(line) {
                                    Err(err) => {
                                        return Err(GraphLoaderError::JsonParseError(format!(
                                            "Error parsing document for line:\n{}\n{:?}",
                                            line, err
                                        )));
                                    }
                                    Ok(val) => val,
                                };

                                let id = &vertex["_id"];
                                let idstr: &String = match id {
                                    Value::String(i) => {
                                        let mut buf = vec![];
                                        buf.extend_from_slice(i[..].as_bytes());
                                        vertex_ids.push(buf);
                                        i
                                    }
                                    _ => {
                                        return Err(GraphLoaderError::JsonParseError(format!(
                                            "JSON is no object with a string _id attribute:\n{}",
                                            vertex
                                        )));
                                    }
                                };

                                if load_config_clone.load_all_vertex_attributes {
                                    vertex.as_object_mut().unwrap().remove("_id");
                                    vertex_json.push(vec![vertex]);
                                } else {
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

                                    let mut cols: Vec<Value> =
                                        Vec::with_capacity(vertex_global_fields.len());
                                    for f in vertex_global_fields.iter() {
                                        let j = get_value(&vertex, f);
                                        cols.push(j);
                                    }

                                    vertex_json.push(cols);
                                }
                            }
                        } else {
                            // This it the AQL Loading variant
                            let values = match serde_json::from_str::<CursorResult>(body) {
                                Err(err) => {
                                    return Err(GraphLoaderError::JsonParseError(format!(
                                        "AQL Error parsing document for body:\n{}\n{:?}",
                                        body, err
                                    )));
                                }
                                Ok(val) => val,
                            };

                            for mut vertex in values.result.into_iter() {
                                let id = &vertex["_id"];
                                let idstr: &String = match id {
                                    Value::String(i) => {
                                        let mut buf = vec![];
                                        buf.extend_from_slice(i[..].as_bytes());
                                        vertex_ids.push(buf);
                                        i
                                    }
                                    _ => {
                                        return Err(GraphLoaderError::JsonParseError(format!(
                                            "JSON is no object with a string _id attribute:\n{}",
                                            vertex
                                        )));
                                    }
                                };

                                if load_config_clone.load_all_vertex_attributes {
                                    vertex.as_object_mut().unwrap().remove("_id");
                                    vertex_json.push(vec![vertex]);
                                } else {
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

                                    let mut cols: Vec<Value> =
                                        Vec::with_capacity(vertex_global_fields.len());
                                    for f in vertex_global_fields.iter() {
                                        let j = get_value(&vertex, f);
                                        cols.push(j);
                                    }
                                    vertex_json.push(cols);
                                }
                            }
                        }
                        insert_vertex_clone(&vertex_ids, &mut vertex_json, &vertex_global_fields)?;
                    }
                    Ok(())
                });
                consumers.push(consumer);
            }

            match &self.load_strategy {
                Some(LoadStrategy::Dump) => {
                    if self.v_collections.is_empty() {
                        error!("No vertex collections given!");
                        return Err(GraphLoaderError::from(
                            "No vertex collections given!".to_string(),
                        ));
                    }
                    if self.vertex_map.is_empty() {
                        error!("No vertex shards found!");
                        return Err(GraphLoaderError::from(
                            "No vertex shards found!".to_string(),
                        ));
                    }
                    let potential_vertex_projections = self.produce_vertex_projections();

                    let dump_result = crate::sharding::get_all_shard_data(
                        &self.db_config,
                        &self.load_config,
                        &self.vertex_map,
                        senders,
                        &self
                            .support_info
                            .as_ref()
                            .unwrap()
                            .deployment
                            .deployment_type,
                        potential_vertex_projections,
                    )
                    .await;
                    if let Err(e) = dump_result {
                        error!("Error fetching vertex data: {:?}", e);
                        return Err(GraphLoaderError::from(format!(
                            "Error fetching vertex data: {:?}",
                            e
                        )));
                    }
                }
                Some(LoadStrategy::Aql) => {
                    let mut v_collection_infos: Vec<CollectionInfo> = vec![];
                    for (_name, info) in self.v_collections.iter() {
                        v_collection_infos.push(info.clone());
                    }

                    let aql_result = get_all_data_aql(
                        &self.db_config,
                        &self.load_config,
                        v_collection_infos.as_slice(),
                        senders,
                        false,
                    )
                    .await;
                    if let Err(e) = aql_result {
                        error!("Error fetching edge data: {:?}", e);
                        return Err(GraphLoaderError::from(format!(
                            "Failed to get aql cursor data: {}",
                            e
                        )));
                    }
                }
                None => {
                    return Err(GraphLoaderError::from("Load strategy not set".to_string()));
                }
            }

            info!("{:?} Got all data, processing...", SystemTime::now());
            for c in consumers {
                let _guck = c.join();
            }
        }
        Ok(())
    }

    pub async fn do_edges<F>(&self, edges_function: F) -> Result<(), GraphLoaderError>
    where
        F: Fn(
                &Vec<Vec<u8>>,
                &Vec<Vec<u8>>,
                &mut Vec<Vec<Value>>,
                &Vec<String>,
            ) -> Result<(), GraphLoaderError>
            + Send
            + Sync
            + Clone
            + 'static,
    {
        let mut senders: Vec<tokio::sync::mpsc::Sender<Bytes>> = vec![];
        let mut consumers: Vec<JoinHandle<Result<(), GraphLoaderError>>> = vec![];

        for _i in 0..self.load_config.parallelism {
            let (sender, mut receiver) = tokio::sync::mpsc::channel::<Bytes>(10);
            senders.push(sender);

            let edge_global_fields = self.get_all_edges_fields_as_list();
            let insert_edge_clone = edges_function.clone();
            let strategy_clone = self.load_strategy;
            let load_config_clone = self.load_config.clone();

            let consumer = std::thread::spawn(move || -> Result<(), GraphLoaderError> {
                while let Some(resp) = receiver.blocking_recv() {
                    let body = std::str::from_utf8(resp.as_ref())
                        .map_err(|e| format!("UTF8 error when parsing body: {:?}", e))?;

                    let mut froms: Vec<Vec<u8>> = Vec::with_capacity(1000000);
                    let mut tos: Vec<Vec<u8>> = Vec::with_capacity(1000000);
                    let mut edge_json: Vec<Vec<Value>> = Vec::with_capacity(400000);

                    if strategy_clone == Option::from(LoadStrategy::Dump) {
                        for line in body.lines() {
                            let mut edge: Value = match serde_json::from_str(line) {
                                Err(err) => {
                                    return Err(GraphLoaderError::from(format!(
                                        "Error parsing document for line:\n{}\n{:?}",
                                        line, err
                                    )));
                                }
                                Ok(val) => val,
                            };

                            let from = &edge["_from"];
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

                            let to = &edge["_to"];
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

                            if load_config_clone.load_all_edge_attributes {
                                edge.as_object_mut().unwrap().remove("_from");
                                edge.as_object_mut().unwrap().remove("_to");
                                edge_json.push(vec![edge]);
                            } else {
                                let id = &edge["_id"];
                                let idstr: &String = match id {
                                    Value::String(i) => i,
                                    _ => {
                                        return Err(GraphLoaderError::JsonParseError(format!(
                                            "JSON is no object with a string _id attribute:\n{}",
                                            line
                                        )));
                                    }
                                };

                                // If we get here, we have to extract the field
                                // values in `fields` from the json and store it
                                // to edge_json:
                                let get_value = |v: &Value, field: &str| -> Value {
                                    if field == "@collection_name" {
                                        Value::String(collection_name_from_id(idstr))
                                    } else {
                                        v[field].clone()
                                    }
                                };

                                let mut cols: Vec<Value> =
                                    Vec::with_capacity(edge_global_fields.len());
                                for f in edge_global_fields.iter() {
                                    let j = get_value(&edge, f);
                                    cols.push(j);
                                }

                                edge_json.push(cols);
                            }
                        }

                        insert_edge_clone(&froms, &tos, &mut edge_json, &edge_global_fields)?;
                    } else {
                        // AQL Variant
                        let values = match serde_json::from_str::<CursorResult>(body) {
                            Err(err) => {
                                return Err(GraphLoaderError::from(format!(
                                    "Error parsing document for body:\n{}\n{:?}",
                                    body, err
                                )));
                            }
                            Ok(val) => val,
                        };

                        for mut edge in values.result.into_iter() {
                            let from = &edge["_from"];
                            match from {
                                Value::String(i) => {
                                    let mut buf = vec![];
                                    buf.extend_from_slice(i[..].as_bytes());
                                    froms.push(buf);
                                }
                                _ => {
                                    return Err(GraphLoaderError::from(format!(
                                        "JSON is no object with a string _from attribute:\n{}",
                                        edge
                                    )));
                                }
                            }
                            let to = &edge["_to"];

                            match to {
                                Value::String(i) => {
                                    let mut buf = vec![];
                                    buf.extend_from_slice(i[..].as_bytes());
                                    tos.push(buf);
                                }
                                _ => {
                                    return Err(GraphLoaderError::from(format!(
                                        "JSON is no object with a string _from attribute:\n{}",
                                        edge
                                    )));
                                }
                            }

                            if load_config_clone.load_all_edge_attributes {
                                edge.as_object_mut().unwrap().remove("_from");
                                edge.as_object_mut().unwrap().remove("_to");
                                edge_json.push(vec![edge]);
                            } else {
                                let id: &Value = &edge["_id"];
                                let idstr: &String = match id {
                                    Value::String(i) => i,
                                    _ => {
                                        return Err(GraphLoaderError::JsonParseError(format!(
                                            "JSON is no object with a string _id attribute:\n{}",
                                            edge
                                        )));
                                    }
                                };

                                // If we get here, we have to extract the field
                                // values in `fields` from the json and store it
                                // to edge_json:
                                let get_value = |v: &Value, field: &str| -> Value {
                                    if field == "@collection_name" {
                                        Value::String(collection_name_from_id(idstr))
                                    } else {
                                        v[field].clone()
                                    }
                                };

                                let mut cols: Vec<Value> =
                                    Vec::with_capacity(edge_global_fields.len());
                                for f in edge_global_fields.iter() {
                                    let j = get_value(&edge, f);
                                    cols.push(j);
                                }
                                edge_json.push(cols);
                            }
                        }

                        insert_edge_clone(&froms, &tos, &mut edge_json, &edge_global_fields)?;
                    }
                }
                Ok(())
            });
            consumers.push(consumer);
        }

        match self.load_strategy {
            Some(LoadStrategy::Dump) => {
                if self.edge_map.is_empty() {
                    error!("No edge shards found!");
                    return Err(GraphLoaderError::from("No edge shards found!".to_string()));
                }
                let potential_edge_projections = self.produce_edge_projections();
                let shard_result = crate::sharding::get_all_shard_data(
                    &self.db_config,
                    &self.load_config,
                    &self.edge_map,
                    senders,
                    &self
                        .support_info
                        .as_ref()
                        .unwrap()
                        .deployment
                        .deployment_type,
                    potential_edge_projections,
                )
                .await;
                if let Err(e) = shard_result {
                    error!("Error fetching edge data: {:?}", e);
                    return Err(e);
                }
            }
            Some(LoadStrategy::Aql) => {
                let mut e_collection_infos: Vec<CollectionInfo> = vec![];
                for (_name, info) in self.e_collections.iter() {
                    e_collection_infos.push(info.clone());
                }

                let aql_result = get_all_data_aql(
                    &self.db_config,
                    &self.load_config,
                    e_collection_infos.as_slice(),
                    senders,
                    true,
                )
                .await;
                if let Err(e) = aql_result {
                    error!("Error fetching edge data: {:?}", e);
                    return Err(GraphLoaderError::from(format!(
                        "Failed to get aql cursor data: {}",
                        e
                    )));
                }
            }
            None => {
                return Err(GraphLoaderError::from("Load strategy not set".to_string()));
            }
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

    pub fn get_all_vertices_fields_as_hashmap(&self) -> HashMap<String, Vec<String>> {
        self.v_collections
            .iter()
            .map(|(k, v)| (k.clone(), v.fields.clone()))
            .collect()
    }

    pub fn get_vertex_fields_as_list(&self, collection_name: &String) -> Vec<String> {
        let mut fields = vec![];
        if let Some(c) = self.v_collections.get(collection_name) {
            fields.extend(c.fields.clone());
        }
        fields
    }

    pub fn get_all_vertices_fields_as_list(&self) -> Vec<String> {
        // Guaranteed to be unique
        let mut unique_fields = HashSet::new();
        for fields in self.v_collections.values().flat_map(|c| c.fields.clone()) {
            unique_fields.insert(fields);
        }

        if unique_fields.is_empty() && !self.load_config.load_all_vertex_attributes {
            unique_fields.insert("_id".to_string());
        }

        unique_fields.into_iter().collect()
    }

    pub fn get_all_edges_fields_as_list(&self) -> Vec<String> {
        // Guaranteed to be unique
        let mut unique_fields = HashSet::new();
        for fields in self.e_collections.values().flat_map(|c| c.fields.clone()) {
            unique_fields.insert(fields);
        }

        if unique_fields.is_empty() && !self.load_config.load_all_edge_attributes {
            unique_fields.insert("_from".to_string());
            unique_fields.insert("_to".to_string());
        }

        unique_fields.into_iter().collect()
    }

    pub fn get_all_edges_fields_as_hashmap(&self) -> HashMap<String, Vec<String>> {
        self.e_collections
            .iter()
            .map(|(k, v)| (k.clone(), v.fields.clone()))
            .collect()
    }

    pub fn produce_vertex_projections(&self) -> Option<HashMap<String, Vec<String>>> {
        let mut potential_vertex_projections: Option<HashMap<String, Vec<String>>> = None;
        let vertex_global_fields = self.get_all_vertices_fields_as_list();

        // We can only make use of projections in case:
        // 1.) The user has not requested all vertex attributes
        // 2.) ArangoDB supports the dump endpoint, which is Version 3.12 or higher
        let client_wants_all_vertex_attributes = self.load_config.load_all_vertex_attributes;
        if !client_wants_all_vertex_attributes {
            let mut vertex_projections: HashMap<String, Vec<String>> = HashMap::new();

            // now add all user specific fields
            for field in vertex_global_fields {
                vertex_projections.insert(field.to_string(), vec![field.to_string()]);
            }
            potential_vertex_projections = Some(vertex_projections);
        }
        potential_vertex_projections
    }

    pub fn produce_edge_projections(&self) -> Option<HashMap<String, Vec<String>>> {
        let mut potential_edge_projections: Option<HashMap<String, Vec<String>>> = None;
        let edge_global_fields = self.get_all_edges_fields_as_list();

        // We can only make use of projections in case:
        // 1.) The user has not requested all vertex attributes
        // 2.) ArangoDB supports the dump endpoint, which is Version 3.12 or higher
        let client_wants_all_edge_attributes = self.load_config.load_all_edge_attributes;
        if !client_wants_all_edge_attributes {
            let mut edge_projections: HashMap<String, Vec<String>> = HashMap::new();

            // if edge_global_fields does not contain "_from" and "_to" we have to add it as it is required.
            // if this is not done, those fields will not be returned from the server's dump endpoint
            if !edge_global_fields.contains(&"_from".to_string()) {
                edge_projections.insert("_from".to_string(), vec!["_from".to_string()]);
            }
            if !edge_global_fields.contains(&"_to".to_string()) {
                edge_projections.insert("_to".to_string(), vec!["_to".to_string()]);
            }
            if !edge_global_fields.contains(&"_id".to_string()) {
                edge_projections.insert("_id".to_string(), vec!["_id".to_string()]);
            }

            for field in edge_global_fields {
                edge_projections.insert(field.to_string(), vec![field.to_string()]);
            }
            potential_edge_projections = Some(edge_projections);
        }
        potential_edge_projections
    }
}

async fn get_graph_collections(
    db_config: &DatabaseConfiguration,
    graph_name: String,
) -> Result<(Vec<String>, Vec<String>), GraphLoaderError> {
    let param_url = format!("/_api/gharial/{}", graph_name);
    let url = make_url(db_config, &param_url);
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

async fn fetch_edge_and_vertex_collections_by_graph(
    db_config: &DatabaseConfiguration,
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

    let resp = handle_auth(client.get(url), db_config).send().await;

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
            let vertex_collection_name = vertex
                .as_str()
                .ok_or(GraphLoaderError::FromCollectionNotString)?;
            non_unique_vertex_collection_names.push(vertex_collection_name.to_string());
        }

        let to = edge_definition["to"]
            .as_array()
            .ok_or(GraphLoaderError::ToNotArray)?;
        for vertex in to {
            let vertex_collection_name = vertex
                .as_str()
                .ok_or(GraphLoaderError::ToCollectionNotString)?;
            non_unique_vertex_collection_names.push(vertex_collection_name.to_string());
        }
    }

    non_unique_vertex_collection_names.sort();
    non_unique_vertex_collection_names.dedup();
    vertex_collection_names.append(&mut non_unique_vertex_collection_names);

    Ok((vertex_collection_names, edge_collection_names))
}
