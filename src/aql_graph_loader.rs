use crate::DatabaseConfiguration;
use crate::client::auth::handle_auth;
use crate::client::config::ClientConfig;
use crate::client::{build_client, make_url};
use crate::errors::GraphLoaderError;
use bytes::Bytes;
use reqwest::StatusCode;
use reqwest_middleware::ClientWithMiddleware;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use tokio::task::JoinSet;

/// Data types supported for graph attributes
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub enum DataType {
    Bool,
    String,
    U64,
    I64,
    F64,
    JSON,
}

/// Description of a data item (attribute) with name and type
#[derive(Clone, Debug)]
pub struct DataItem {
    pub name: String,
    pub data_type: DataType,
}

/// A batch of graph data containing vertices and edges
#[derive(Debug)]
pub struct GraphBatch {
    /// Vertex IDs as byte vectors
    pub vertex_ids: Vec<Vec<u8>>,
    /// Vertex attributes, parallel to vertex_ids
    pub vertex_attributes: Vec<Vec<Value>>,
    /// Edge source IDs as byte vectors
    pub edge_from_ids: Vec<Vec<u8>>,
    /// Edge target IDs as byte vectors
    pub edge_to_ids: Vec<Vec<u8>>,
    /// Edge attributes, parallel to edge_from_ids/edge_to_ids
    pub edge_attributes: Vec<Vec<Value>>,
    /// Total number of type conversion errors encountered
    pub type_error_count: usize,
    /// First few type error messages (up to 10)
    pub type_error_messages: Vec<String>,
}

impl DataItem {
    pub fn new(name: String, data_type: DataType) -> Self {
        DataItem { name, data_type }
    }
}

impl GraphBatch {
    fn new() -> Self {
        GraphBatch {
            vertex_ids: Vec::new(),
            vertex_attributes: Vec::new(),
            edge_from_ids: Vec::new(),
            edge_to_ids: Vec::new(),
            edge_attributes: Vec::new(),
            type_error_count: 0,
            type_error_messages: Vec::new(),
        }
    }

    fn add_type_error(&mut self, message: String) {
        self.type_error_count += 1;
        if self.type_error_messages.len() < 10 {
            self.type_error_messages.push(message);
        }
    }
}

/// Convert and validate a value according to the expected data type
fn convert_and_validate(
    value: &Value,
    expected_type: &DataType,
    attr_name: &str,
    entity_id: &str,
) -> Result<Value, String> {
    match expected_type {
        DataType::Bool => {
            if let Some(b) = value.as_bool() {
                Ok(Value::Bool(b))
            } else if let Some(s) = value.as_str() {
                // Try to parse string as bool
                match s.to_lowercase().as_str() {
                    "true" | "1" | "yes" => Ok(Value::Bool(true)),
                    "false" | "0" | "no" => Ok(Value::Bool(false)),
                    _ => Err(format!(
                        "Cannot convert '{}' to bool for attribute '{}' in entity '{}'",
                        s, attr_name, entity_id
                    )),
                }
            } else if let Some(n) = value.as_i64() {
                Ok(Value::Bool(n != 0))
            } else if let Some(n) = value.as_u64() {
                Ok(Value::Bool(n != 0))
            } else if let Some(n) = value.as_f64() {
                Ok(Value::Bool(n != 0.0))
            } else {
                Err(format!(
                    "Cannot convert {:?} to bool for attribute '{}' in entity '{}'",
                    value, attr_name, entity_id
                ))
            }
        }
        DataType::String => {
            if let Some(s) = value.as_str() {
                Ok(Value::String(s.to_string()))
            } else if value.is_null() {
                Ok(Value::String(String::new()))
            } else if let Some(b) = value.as_bool() {
                Ok(Value::String(b.to_string()))
            } else if let Some(n) = value.as_i64() {
                Ok(Value::String(n.to_string()))
            } else if let Some(n) = value.as_u64() {
                Ok(Value::String(n.to_string()))
            } else if let Some(n) = value.as_f64() {
                Ok(Value::String(n.to_string()))
            } else {
                // For objects/arrays, convert to JSON string
                Ok(Value::String(value.to_string()))
            }
        }
        DataType::U64 => {
            if let Some(n) = value.as_u64() {
                Ok(Value::Number(n.into()))
            } else if let Some(n) = value.as_i64() {
                if n >= 0 {
                    Ok(Value::Number((n as u64).into()))
                } else {
                    Err(format!(
                        "Cannot convert negative number {} to u64 for attribute '{}' in entity '{}'",
                        n, attr_name, entity_id
                    ))
                }
            } else if let Some(f) = value.as_f64() {
                if f >= 0.0 && f <= u64::MAX as f64 {
                    Ok(Value::Number((f.round() as u64).into()))
                } else {
                    Err(format!(
                        "Cannot convert {} to u64 for attribute '{}' in entity '{}'",
                        f, attr_name, entity_id
                    ))
                }
            } else if let Some(s) = value.as_str() {
                s.parse::<u64>()
                    .map(|n| Value::Number(n.into()))
                    .map_err(|_| {
                        format!(
                            "Cannot parse '{}' as u64 for attribute '{}' in entity '{}'",
                            s, attr_name, entity_id
                        )
                    })
            } else {
                Err(format!(
                    "Cannot convert {:?} to u64 for attribute '{}' in entity '{}'",
                    value, attr_name, entity_id
                ))
            }
        }
        DataType::I64 => {
            if let Some(n) = value.as_i64() {
                Ok(Value::Number(n.into()))
            } else if let Some(n) = value.as_u64() {
                if n <= i64::MAX as u64 {
                    Ok(Value::Number((n as i64).into()))
                } else {
                    Err(format!(
                        "Cannot convert {} to i64 (overflow) for attribute '{}' in entity '{}'",
                        n, attr_name, entity_id
                    ))
                }
            } else if let Some(f) = value.as_f64() {
                if f >= i64::MIN as f64 && f <= i64::MAX as f64 {
                    Ok(Value::Number((f.round() as i64).into()))
                } else {
                    Err(format!(
                        "Cannot convert {} to i64 for attribute '{}' in entity '{}'",
                        f, attr_name, entity_id
                    ))
                }
            } else if let Some(s) = value.as_str() {
                s.parse::<i64>()
                    .map(|n| Value::Number(n.into()))
                    .map_err(|_| {
                        format!(
                            "Cannot parse '{}' as i64 for attribute '{}' in entity '{}'",
                            s, attr_name, entity_id
                        )
                    })
            } else {
                Err(format!(
                    "Cannot convert {:?} to i64 for attribute '{}' in entity '{}'",
                    value, attr_name, entity_id
                ))
            }
        }
        DataType::F64 => {
            if let Some(f) = value.as_f64() {
                Ok(Value::Number(
                    serde_json::Number::from_f64(f).unwrap_or_else(|| 0.into()),
                ))
            } else if let Some(n) = value.as_i64() {
                Ok(Value::Number(
                    serde_json::Number::from_f64(n as f64).unwrap_or_else(|| 0.into()),
                ))
            } else if let Some(n) = value.as_u64() {
                Ok(Value::Number(
                    serde_json::Number::from_f64(n as f64).unwrap_or_else(|| 0.into()),
                ))
            } else if let Some(s) = value.as_str() {
                s.parse::<f64>()
                    .map(|f| {
                        Value::Number(serde_json::Number::from_f64(f).unwrap_or_else(|| 0.into()))
                    })
                    .map_err(|_| {
                        format!(
                            "Cannot parse '{}' as f64 for attribute '{}' in entity '{}'",
                            s, attr_name, entity_id
                        )
                    })
            } else {
                Err(format!(
                    "Cannot convert {:?} to f64 for attribute '{}' in entity '{}'",
                    value, attr_name, entity_id
                ))
            }
        }
        DataType::JSON => {
            // JSON type accepts anything
            Ok(value.clone())
        }
    }
}

/// Get default value for a data type
fn default_value_for_type(data_type: &DataType) -> Value {
    match data_type {
        DataType::Bool => Value::Bool(false),
        DataType::String => Value::String(String::new()),
        DataType::U64 => Value::Number(0.into()),
        DataType::I64 => Value::Number(0.into()),
        DataType::F64 => Value::Number(serde_json::Number::from_f64(0.0).unwrap()),
        DataType::JSON => Value::Null,
    }
}

/// An AQL query with bind variables
#[derive(Clone, Debug)]
pub struct AqlQuery {
    pub query: String,
    pub bind_vars: HashMap<String, Value>,
}

impl AqlQuery {
    pub fn new(query: String, bind_vars: HashMap<String, Value>) -> Self {
        AqlQuery { query, bind_vars }
    }
}

/// AQL-based graph loader
pub struct AqlGraphLoader {
    db_config: DatabaseConfiguration,
    batch_size: u64,
    vertex_attributes: Vec<DataItem>,
    edge_attributes: Vec<DataItem>,
    queries: Vec<Vec<AqlQuery>>,
}

#[derive(Debug, Serialize, Deserialize)]
struct CursorOptions {
    stream: bool,
}

impl CursorOptions {
    pub fn new(stream: bool) -> Self {
        Self { stream }
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CreateCursorBody {
    query: String,
    options: CursorOptions,
    #[serde(skip_serializing_if = "Option::is_none")]
    batch_size: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    bind_vars: Option<HashMap<String, Value>>,
}

impl CreateCursorBody {
    pub fn from_streaming_query_with_size(
        query: String,
        batch_size: Option<u64>,
        bind_vars: Option<HashMap<String, Value>>,
    ) -> Self {
        Self {
            query,
            batch_size,
            options: CursorOptions::new(true),
            bind_vars,
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CursorResponse {
    has_more: Option<bool>,
    id: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct GraphData {
    vertices: Option<Vec<Value>>,
    edges: Option<Vec<Value>>,
}

impl AqlGraphLoader {
    /// Create a new AQL graph loader
    pub async fn new(
        db_config: DatabaseConfiguration,
        batch_size: u64,
        vertex_attributes: Vec<DataItem>,
        edge_attributes: Vec<DataItem>,
        queries: Vec<Vec<AqlQuery>>,
    ) -> Result<Self, GraphLoaderError> {
        // Validate that we have at least one query
        if queries.is_empty() || queries.iter().all(|q| q.is_empty()) {
            return Err(GraphLoaderError::Other(
                "At least one AQL query must be provided".to_string(),
            ));
        }

        Ok(AqlGraphLoader {
            db_config,
            batch_size,
            vertex_attributes,
            edge_attributes,
            queries,
        })
    }

    /// Execute all AQL queries and call the provided callback with vertices and edges
    pub async fn do_load<F>(&self, callback: F) -> Result<(), GraphLoaderError>
    where
        F: Fn(&mut GraphBatch) -> Result<(), GraphLoaderError> + Send + Sync + Clone + 'static,
    {
        // Build the client
        let use_tls = self.db_config.endpoints[0].starts_with("https://");
        let client_config = ClientConfig::builder()
            .n_retries(5)
            .use_tls(use_tls)
            .tls_cert_opt(self.db_config.tls_cert.clone())
            .build();
        let client = build_client(&client_config)?;

        // Process each group of queries sequentially
        for query_group in &self.queries {
            self.execute_query_group(&client, query_group, callback.clone())
                .await?;
        }

        Ok(())
    }

    async fn execute_query_group<F>(
        &self,
        client: &ClientWithMiddleware,
        queries: &[AqlQuery],
        callback: F,
    ) -> Result<(), GraphLoaderError>
    where
        F: Fn(&mut GraphBatch) -> Result<(), GraphLoaderError> + Send + Sync + Clone + 'static,
    {
        // Create a channel for receiving data
        let (sender, mut receiver) = tokio::sync::mpsc::channel::<Bytes>(10);

        // Spawn consumer thread
        let callback_clone = callback.clone();
        let vertex_attributes = self.vertex_attributes.clone();
        let edge_attributes = self.edge_attributes.clone();

        let consumer = std::thread::spawn(move || -> Result<(), GraphLoaderError> {
            while let Some(resp) = receiver.blocking_recv() {
                let body = std::str::from_utf8(resp.as_ref())
                    .map_err(|e| format!("UTF8 error when parsing body: {:?}", e))?;

                // Parse the cursor response
                let cursor_result: serde_json::Result<CursorResponse> = serde_json::from_str(body);
                if cursor_result.is_err() {
                    return Err(GraphLoaderError::ParseError(format!(
                        "Failed to parse cursor response: {:?}",
                        cursor_result.err()
                    )));
                }

                // Parse the actual result data
                let parsed: serde_json::Result<serde_json::Map<String, Value>> =
                    serde_json::from_str(body);
                if parsed.is_err() {
                    return Err(GraphLoaderError::ParseError(format!(
                        "Failed to parse result data: {:?}",
                        parsed.err()
                    )));
                }

                let data = parsed.unwrap();
                let result = data.get("result");
                if result.is_none() {
                    continue;
                }

                let result_array = result.unwrap().as_array();
                if result_array.is_none() {
                    continue;
                }

                // Process each item in the result
                for item in result_array.unwrap() {
                    let graph_data: serde_json::Result<GraphData> =
                        serde_json::from_value(item.clone());
                    if let Ok(graph_data) = graph_data {
                        let mut batch = GraphBatch::new();

                        // Extract and validate vertices
                        if let Some(vertices) = graph_data.vertices {
                            for vertex in vertices {
                                if let Some(id) = vertex.get("_id")
                                    && let Some(id_str) = id.as_str()
                                {
                                    batch.vertex_ids.push(id_str.as_bytes().to_vec());

                                    // Extract and validate attributes
                                    let mut attrs = Vec::new();
                                    for attr_def in &vertex_attributes {
                                        let raw_value = vertex
                                            .get(&attr_def.name)
                                            .cloned()
                                            .unwrap_or(Value::Null);

                                        match convert_and_validate(
                                            &raw_value,
                                            &attr_def.data_type,
                                            &attr_def.name,
                                            id_str,
                                        ) {
                                            Ok(converted) => attrs.push(converted),
                                            Err(err_msg) => {
                                                batch.add_type_error(err_msg);
                                                attrs.push(default_value_for_type(
                                                    &attr_def.data_type,
                                                ));
                                            }
                                        }
                                    }
                                    batch.vertex_attributes.push(attrs);
                                }
                            }
                        }

                        // Extract and validate edges
                        if let Some(edges) = graph_data.edges {
                            for edge in edges {
                                // Skip null edges (e.g., from traversal start node)
                                if edge.is_null() {
                                    continue;
                                }

                                if let (Some(from), Some(to)) = (edge.get("_from"), edge.get("_to"))
                                    && let (Some(from_str), Some(to_str)) =
                                        (from.as_str(), to.as_str())
                                {
                                    let edge_id = format!("{}-->{}", from_str, to_str);
                                    batch.edge_from_ids.push(from_str.as_bytes().to_vec());
                                    batch.edge_to_ids.push(to_str.as_bytes().to_vec());

                                    // Extract and validate attributes
                                    let mut attrs = Vec::new();
                                    for attr_def in &edge_attributes {
                                        let raw_value = edge
                                            .get(&attr_def.name)
                                            .cloned()
                                            .unwrap_or(Value::Null);

                                        match convert_and_validate(
                                            &raw_value,
                                            &attr_def.data_type,
                                            &attr_def.name,
                                            &edge_id,
                                        ) {
                                            Ok(converted) => attrs.push(converted),
                                            Err(err_msg) => {
                                                batch.add_type_error(err_msg);
                                                attrs.push(default_value_for_type(
                                                    &attr_def.data_type,
                                                ));
                                            }
                                        }
                                    }
                                    batch.edge_attributes.push(attrs);
                                }
                            }
                        }

                        // Call the callback with the batch
                        callback_clone(&mut batch)?;
                    }
                }
            }
            Ok(())
        });

        // Execute all queries in parallel
        let mut task_set = JoinSet::new();

        for query in queries {
            let client_clone = client.clone();
            let db_config = self.db_config.clone();
            let query_clone = query.clone();
            let batch_size = self.batch_size;
            let sender_clone = sender.clone();

            task_set.spawn(async move {
                Self::execute_single_query(
                    &client_clone,
                    &db_config,
                    &query_clone,
                    batch_size,
                    sender_clone,
                )
                .await
            });
        }

        // Drop the original sender so the receiver can finish
        drop(sender);

        // Wait for all tasks to complete
        let mut errors: Vec<String> = Vec::new();
        while let Some(res) = task_set.join_next().await {
            match res {
                Ok(Ok(())) => {}
                Ok(Err(e)) => {
                    errors.push(e.to_string());
                }
                Err(e) => {
                    errors.push(format!("Task join error: {}", e));
                }
            }
        }

        // Wait for consumer to finish
        let consumer_result = consumer
            .join()
            .map_err(|_| GraphLoaderError::Other("Consumer thread panicked".to_string()))?;

        if !errors.is_empty() {
            return Err(GraphLoaderError::Other(format!(
                "Errors occurred during query execution: {}",
                errors.join("; ")
            )));
        }

        consumer_result
    }

    async fn execute_single_query(
        client: &ClientWithMiddleware,
        db_config: &DatabaseConfiguration,
        query: &AqlQuery,
        batch_size: u64,
        sender: tokio::sync::mpsc::Sender<Bytes>,
    ) -> Result<(), GraphLoaderError> {
        let make_cursor_url = |path: &str| -> String {
            let suffix = "/_api/cursor".to_owned() + path;
            make_url(db_config, suffix.as_str())
        };

        // Create cursor
        let body = CreateCursorBody::from_streaming_query_with_size(
            query.query.clone(),
            Some(batch_size),
            Some(query.bind_vars.clone()),
        );
        let body_v = serde_json::to_vec::<CreateCursorBody>(&body).map_err(|e| {
            GraphLoaderError::ParseError(format!("Failed to serialize body: {}", e))
        })?;

        let url = make_cursor_url("");
        let cursor_create_resp = handle_auth(client.post(url), db_config)
            .body(body_v)
            .send()
            .await;

        let response = cursor_create_resp?;
        let bytes_res = response
            .bytes()
            .await
            .map_err(|e| GraphLoaderError::ParseError(format!("Error reading response: {}", e)))?;

        let response_info = serde_json::from_slice::<CursorResponse>(&bytes_res)
            .map_err(|e| GraphLoaderError::ParseError(format!("Failed to parse cursor: {}", e)))?;

        // Send first batch
        sender
            .send(bytes_res)
            .await
            .map_err(|e| GraphLoaderError::Other(format!("Failed to send data: {}", e)))?;

        // Fetch remaining batches if needed
        if let Some(cursor_id) = response_info.id
            && response_info.has_more.unwrap_or(false)
        {
            loop {
                let url = make_cursor_url(&format!("/{}", cursor_id));
                let resp = handle_auth(client.post(url), db_config).send().await;

                let resp =
                    crate::request::handle_arangodb_response(resp, |c| c == StatusCode::OK).await?;

                let bytes_res = resp.bytes().await.map_err(|e| {
                    GraphLoaderError::ParseError(format!("Error reading response: {}", e))
                })?;

                let response_info =
                    serde_json::from_slice::<CursorResponse>(&bytes_res).map_err(|e| {
                        GraphLoaderError::ParseError(format!("Failed to parse cursor: {}", e))
                    })?;

                sender
                    .send(bytes_res)
                    .await
                    .map_err(|e| GraphLoaderError::Other(format!("Failed to send data: {}", e)))?;

                if !response_info.has_more.unwrap_or(false) {
                    break;
                }
            }

            // Clean up cursor
            let delete_url = make_cursor_url(&format!("/{}", cursor_id));
            let _ = handle_auth(client.delete(delete_url), db_config)
                .send()
                .await;
        }

        Ok(())
    }
}
