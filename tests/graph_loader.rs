use arangors_graph_exporter::{
    CollectionInfo, DataLoadConfiguration, DataLoadConfigurationBuilder, DatabaseConfiguration,
    DatabaseConfigurationBuilder, GraphLoader,
};
use serial_test::serial;

use arangors_graph_exporter::aql_graph_loader::{AqlGraphLoader, AqlQuery, DataItem, DataType};
use arangors_graph_exporter::client::config::ClientConfig;
use arangors_graph_exporter::errors::GraphLoaderError;
use rstest::fixture;
use serde_json::{Value, json};
use std::collections::{HashMap, HashSet};
use std::env;
use std::sync::{Arc, Mutex};

static GRAPH: &str = "IntegrationTestGraph";
static EDGE_COLLECTION: &str = "IntegrationTestEdge";
static VERTEX_COLLECTION: &str = "IntegrationTestVertex";
static USERNAME: &str = "root";
static PASSWORD: &str = "test";
static DATABASE: &str = "_system";

fn is_ssl_enabled() -> bool {
    env::var("SSL")
        .unwrap_or_else(|_| "FALSE".to_string())
        .to_uppercase()
        == "TRUE"
}

fn get_db_url() -> String {
    if let Ok(url) = env::var("ARANGODB_DB_URL") {
        return url;
    }

    let protocol = if is_ssl_enabled() { "https" } else { "http" };
    format!("{}://localhost:8529", protocol)
}

// Helper to create an HTTP client for test setup
fn build_test_client() -> reqwest_middleware::ClientWithMiddleware {
    let db_config = build_db_config();
    let use_tls = db_config.endpoints[0].starts_with("https://");
    let client_config = ClientConfig::builder()
        .n_retries(3)
        .use_tls(use_tls)
        .tls_cert_opt(db_config.tls_cert.clone())
        .build();
    arangors_graph_exporter::client::build_client(&client_config).unwrap()
}

fn build_db_config() -> DatabaseConfiguration {
    let endpoints = vec![get_db_url()];
    let mut db_config_builder = DatabaseConfigurationBuilder::new()
        .endpoints(endpoints)
        .username(USERNAME.to_string())
        .password(PASSWORD.to_string())
        .database(DATABASE.to_string());

    // Add TLS certificate if SSL is enabled and a certificate path is provided
    // If no certificate is provided, the client will accept self-signed certificates
    if is_ssl_enabled() && env::var("TLS_CERT_PATH").is_ok() {
        db_config_builder = db_config_builder.tls_cert(env::var("TLS_CERT_PATH").unwrap());
    }
    // If TLS_CERT_PATH is not set, no certificate is added,
    // and danger_accept_invalid_certs will be used

    db_config_builder.build()
}

fn build_load_config() -> DataLoadConfiguration {
    DataLoadConfigurationBuilder::new()
        .parallelism(8)
        .batch_size(100000)
        .build()
}

fn build_load_config_with_v_with_e(
    fetch_all_v_attributes: bool,
    fetch_all_e_attributes: bool,
) -> DataLoadConfiguration {
    DataLoadConfigurationBuilder::new()
        .parallelism(8)
        .batch_size(100000)
        .load_all_vertex_attributes(fetch_all_v_attributes)
        .load_all_edge_attributes(fetch_all_e_attributes)
        .build()
}

async fn is_cluster() -> bool {
    let db_config = build_db_config();
    let client = build_test_client();

    let url = format!("{}/_admin/server/role", db_config.endpoints[0]);
    let resp = client
        .get(&url)
        .basic_auth(USERNAME, Some(PASSWORD))
        .send()
        .await
        .unwrap();

    let json: Value = resp.json().await.unwrap();
    json["role"].as_str().unwrap_or("") == "COORDINATOR"
}

#[derive(Debug)]
#[allow(dead_code)]
struct Version {
    pub version: String,
    pub license: String,
    pub server: String,
}

async fn get_arangodb_version() -> Version {
    let db_config = build_db_config();
    let client = build_test_client();

    let url = format!("{}/_api/version", db_config.endpoints[0]);
    let resp = client
        .get(&url)
        .basic_auth(USERNAME, Some(PASSWORD))
        .send()
        .await
        .unwrap();

    let json: Value = resp.json().await.unwrap();
    Version {
        version: json["version"].as_str().unwrap().to_string(),
        license: json["license"].as_str().unwrap().to_string(),
        server: json["server"].as_str().unwrap().to_string(),
    }
}

fn extract_version_parts(version: &str) -> Result<(u32, u32, u32), &'static str> {
    let version_part = version.split('-').next().ok_or("Invalid version string")?;
    let mut parts = version_part.split('.');

    let major = parts
        .next()
        .ok_or("Missing major version")?
        .parse()
        .map_err(|_| "Invalid major version")?;
    let minor = parts
        .next()
        .ok_or("Missing minor version")?
        .parse()
        .map_err(|_| "Invalid minor version")?;
    let patch = parts
        .next()
        .ok_or("Missing patch version")?
        .parse()
        .map_err(|_| "Invalid patch version")?;

    Ok((major, minor, patch))
}

async fn create_graph(insert_data: bool) {
    let db_config = build_db_config();
    let client = build_test_client();

    // Drop graph if exists
    let drop_url = format!("{}/_api/gharial/{}", db_config.endpoints[0], GRAPH);
    let _ = client
        .delete(&drop_url)
        .basic_auth(USERNAME, Some(PASSWORD))
        .query(&[("dropCollections", "true")])
        .send()
        .await;

    // Create graph
    let create_graph_body = json!({
        "name": GRAPH,
        "edgeDefinitions": [{
            "collection": EDGE_COLLECTION,
            "from": [VERTEX_COLLECTION],
            "to": [VERTEX_COLLECTION]
        }],
        "orphanCollections": []
    });

    let create_url = format!("{}/_api/gharial", db_config.endpoints[0]);
    let resp = client
        .post(&create_url)
        .basic_auth(USERNAME, Some(PASSWORD))
        .header("Content-Type", "application/json")
        .body(serde_json::to_string(&create_graph_body).unwrap())
        .send()
        .await
        .unwrap();

    assert!(
        resp.status().is_success(),
        "Failed to create graph: status={}",
        resp.status()
    );

    if insert_data {
        // Insert vertices
        let vertex_url = format!(
            "{}/_api/document/{}",
            db_config.endpoints[0], VERTEX_COLLECTION
        );
        for i in 0..10 {
            let doc = json!({
                "_key": i.to_string(),
                "x": i + 1,
                "y": i + 2,
                "z": i + 3
            });
            client
                .post(&vertex_url)
                .basic_auth(USERNAME, Some(PASSWORD))
                .header("Content-Type", "application/json")
                .body(serde_json::to_string(&doc).unwrap())
                .send()
                .await
                .unwrap();
        }

        // Insert edges
        let edge_url = format!(
            "{}/_api/document/{}",
            db_config.endpoints[0], EDGE_COLLECTION
        );
        for i in 0..9 {
            let doc = json!({
                "_key": i.to_string(),
                "_from": format!("{}/{}", VERTEX_COLLECTION, i),
                "_to": format!("{}/{}", VERTEX_COLLECTION, i + 1),
                "x": i + 1,
                "y": i + 2,
                "z": i + 3
            });
            client
                .post(&edge_url)
                .basic_auth(USERNAME, Some(PASSWORD))
                .header("Content-Type", "application/json")
                .body(serde_json::to_string(&doc).unwrap())
                .send()
                .await
                .unwrap();
        }
    }
}

async fn drop_graph() {
    let db_config = build_db_config();
    let client = build_test_client();

    let drop_url = format!("{}/_api/gharial/{}", db_config.endpoints[0], GRAPH);
    let _ = client
        .delete(&drop_url)
        .basic_auth(USERNAME, Some(PASSWORD))
        .query(&[("dropCollections", "true")])
        .send()
        .await;
}

async fn setup(insert_data: bool) {
    create_graph(insert_data).await;
}

async fn teardown() {
    drop_graph().await;
}

#[tokio::test]
#[serial]
async fn init_named_graph_loader() {
    setup(false).await;
    let db_config = build_db_config();
    let load_config = build_load_config();

    let graph_loader_res =
        GraphLoader::new_named(db_config, load_config, GRAPH.to_string(), None, None).await;
    if let Err(ref e) = graph_loader_res {
        println!("{:?}", e);
    }

    assert!(graph_loader_res.is_ok());
    teardown().await;
}

#[tokio::test]
#[serial]
async fn init_named_graph_loader_with_data() {
    setup(true).await;
    let db_config = build_db_config();
    let load_config = build_load_config();

    let graph_loader_res =
        GraphLoader::new_named(db_config, load_config, GRAPH.to_string(), None, None).await;
    if let Err(ref e) = graph_loader_res {
        println!("{:?}", e);
    }

    assert!(graph_loader_res.is_ok());

    // check that data is loaded.
    // in this case, no global vertex attributes have been requested
    // therefore only the _id attribute is loaded for vertex documents
    let graph_loader = graph_loader_res.unwrap();
    let handle_vertices = move |vertex_ids: &Vec<Vec<u8>>,
                                columns: &mut Vec<Vec<Value>>,
                                vertex_field_names: &Vec<String>| {
        assert_eq!(vertex_ids.len(), 10);

        assert_eq!(columns.len(), 10);
        for vertex in columns.iter() {
            assert_eq!(vertex.len(), 0);
            assert_eq!(vertex.len(), vertex_field_names.len());
        }

        assert_eq!(vertex_field_names.len(), 0);
        Ok(())
    };
    let vertices_result = graph_loader.do_vertices(handle_vertices).await;
    assert!(vertices_result.is_ok());

    let handle_edges = move |from_ids: &Vec<Vec<u8>>,
                             to_ids: &Vec<Vec<u8>>,
                             columns: &mut Vec<Vec<Value>>,
                             edge_field_names: &Vec<String>| {
        assert_eq!(from_ids.len(), 9);
        assert_eq!(from_ids.len(), to_ids.len());
        assert_eq!(to_ids.len(), 9);
        assert_eq!(to_ids.len(), columns.len());
        assert_eq!(columns.len(), 9);

        for (from_idx, from_id) in from_ids.iter().enumerate() {
            let from_id_str = from_id.iter().map(|x| *x as char).collect::<String>();
            let expected_from_id = format!("{}/{}", VERTEX_COLLECTION, from_idx);
            assert_eq!(from_id_str, expected_from_id);
        }
        for (to_idx, to_id) in to_ids.iter().enumerate() {
            let to_id_str = to_id.iter().map(|x| *x as char).collect::<String>();
            let expected_to_id = format!("{}/{}", VERTEX_COLLECTION, to_idx + 1);
            assert_eq!(to_id_str, expected_to_id);
        }

        assert_eq!(edge_field_names.len(), 0);
        Ok(())
    };
    let edges_result = graph_loader.do_edges(handle_edges).await;
    assert!(edges_result.is_ok());

    teardown().await;
}

fn get_attribute_position_from_fields(field_names: &[String], attribute: &str) -> usize {
    assert!(!field_names.is_empty());
    assert!(field_names.contains(&attribute.to_string()));
    field_names.iter().position(|x| x == attribute).unwrap()
}

#[tokio::test]
#[serial]
async fn init_named_graph_loader_with_data_all_v_and_e_attributes_manually_set() {
    setup(true).await;
    let db_config = build_db_config();
    let load_config = build_load_config();
    let global_fields = vec!["x".to_string(), "y".to_string(), "z".to_string()];
    let graph_loader_res = GraphLoader::new_named(
        db_config,
        load_config,
        GRAPH.to_string(),
        Some(global_fields.clone()),
        Some(global_fields),
    )
    .await;
    if let Err(ref e) = graph_loader_res {
        println!("{:?}", e);
    }

    assert!(graph_loader_res.is_ok());

    // check that data is loaded.
    // in this case, no global vertex attributes have been requested
    // therefore only the _key attribute is loaded for vertex documents
    let graph_loader = graph_loader_res.unwrap();
    let handle_vertices = move |vertex_ids: &Vec<Vec<u8>>,
                                columns: &mut Vec<Vec<Value>>,
                                vertex_field_names: &Vec<String>| {
        assert_eq!(vertex_ids.len(), 10);
        assert_eq!(columns.len(), 10);

        for (v_index, v_id) in vertex_ids.iter().enumerate() {
            let id = v_id.iter().map(|x| *x as char).collect::<String>();
            let expected_id = format!("{}/{}", VERTEX_COLLECTION, v_index);
            assert_eq!(id, expected_id);
        }

        for (v_index, vertex) in columns.iter().enumerate() {
            assert_eq!(vertex.len(), 3);
            assert_eq!(vertex.len(), vertex_field_names.len());

            let x = &vertex[get_attribute_position_from_fields(vertex_field_names, "x")]
                .as_u64()
                .unwrap();
            let y = &vertex[get_attribute_position_from_fields(vertex_field_names, "y")]
                .as_u64()
                .unwrap();
            let z = &vertex[get_attribute_position_from_fields(vertex_field_names, "z")]
                .as_u64()
                .unwrap();
            let expected_x_value = (v_index + 1) as u64;
            let expected_y_value = (v_index + 2) as u64;
            let expected_z_value = (v_index + 3) as u64;
            assert_eq!(x, &expected_x_value);
            assert_eq!(y, &expected_y_value);
            assert_eq!(z, &expected_z_value);
        }

        assert_eq!(vertex_field_names.len(), 3);
        assert!(vertex_field_names.contains(&"x".to_string()));
        assert!(vertex_field_names.contains(&"y".to_string()));
        assert!(vertex_field_names.contains(&"z".to_string()));
        Ok(())
    };
    let vertices_result = graph_loader.do_vertices(handle_vertices).await;
    assert!(vertices_result.is_ok());

    let handle_edges = move |from_ids: &Vec<Vec<u8>>,
                             to_ids: &Vec<Vec<u8>>,
                             columns: &mut Vec<Vec<Value>>,
                             edge_field_names: &Vec<String>| {
        assert_eq!(from_ids.len(), 9);
        assert_eq!(from_ids.len(), to_ids.len());
        assert_eq!(columns.len(), 9);

        for (e_index, from_id) in from_ids.iter().enumerate() {
            let from_id_str = from_id.iter().map(|x| *x as char).collect::<String>();
            let to_id_str = to_ids[e_index]
                .iter()
                .map(|x| *x as char)
                .collect::<String>();
            assert_eq!(from_id_str, format!("{}/{}", VERTEX_COLLECTION, e_index));
            assert_eq!(to_id_str, format!("{}/{}", VERTEX_COLLECTION, e_index + 1));
        }

        for (e_index, to_id) in to_ids.iter().enumerate() {
            let from_id_str = from_ids[e_index]
                .iter()
                .map(|x| *x as char)
                .collect::<String>();
            let to_id_str = to_id.iter().map(|x| *x as char).collect::<String>();
            assert_eq!(from_id_str, format!("{}/{}", VERTEX_COLLECTION, e_index));
            assert_eq!(to_id_str, format!("{}/{}", VERTEX_COLLECTION, e_index + 1));
        }

        for (e_index, edge) in columns.iter().enumerate() {
            assert_eq!(edge.len(), 3);
            assert_eq!(edge.len(), edge_field_names.len());

            let x = &edge[get_attribute_position_from_fields(edge_field_names, "x")]
                .as_u64()
                .unwrap();
            let y = &edge[get_attribute_position_from_fields(edge_field_names, "y")]
                .as_u64()
                .unwrap();
            let z = &edge[get_attribute_position_from_fields(edge_field_names, "z")]
                .as_u64()
                .unwrap();
            let expected_x_value = (e_index + 1) as u64;
            let expected_y_value = (e_index + 2) as u64;
            let expected_z_value = (e_index + 3) as u64;
            assert_eq!(x, &expected_x_value);
            assert_eq!(y, &expected_y_value);
            assert_eq!(z, &expected_z_value);
        }

        assert_eq!(edge_field_names.len(), 3);
        assert!(edge_field_names.contains(&"x".to_string()));
        assert!(edge_field_names.contains(&"y".to_string()));
        assert!(edge_field_names.contains(&"z".to_string()));
        Ok(())
    };
    let edges_result = graph_loader.do_edges(handle_edges).await;
    assert!(edges_result.is_ok());

    teardown().await;
}

#[tokio::test]
#[serial]
async fn init_named_graph_loader_with_data_all_v_and_e_collection_name_attribute() {
    setup(true).await;
    let db_config = build_db_config();
    let load_config = build_load_config();
    let global_fields = vec!["@collection_name".to_string()];
    let graph_loader_res = GraphLoader::new_named(
        db_config,
        load_config,
        GRAPH.to_string(),
        Some(global_fields.clone()),
        Some(global_fields),
    )
    .await;
    if let Err(ref e) = graph_loader_res {
        println!("{:?}", e);
    }

    assert!(graph_loader_res.is_ok());

    // check that data is loaded.
    // in this case, no global vertex attributes have been requested
    // therefore only the _key attribute is loaded for vertex documents
    let graph_loader = graph_loader_res.unwrap();
    let handle_vertices = move |vertex_ids: &Vec<Vec<u8>>,
                                columns: &mut Vec<Vec<Value>>,
                                vertex_field_names: &Vec<String>| {
        assert_eq!(vertex_ids.len(), 10);
        assert_eq!(columns.len(), 10);

        for (v_index, v_id) in vertex_ids.iter().enumerate() {
            let id = v_id.iter().map(|x| *x as char).collect::<String>();
            let expected_id = format!("{}/{}", VERTEX_COLLECTION, v_index);
            assert_eq!(id, expected_id);
        }

        for vertex in columns.iter() {
            assert_eq!(vertex.len(), 1);
            assert_eq!(vertex.len(), vertex_field_names.len());

            let collection_name = &vertex
                [get_attribute_position_from_fields(vertex_field_names, "@collection_name")]
            .as_str()
            .unwrap();
            let expected_collection_name = VERTEX_COLLECTION;
            assert_eq!(collection_name, &expected_collection_name);
        }

        assert_eq!(vertex_field_names.len(), 1);
        assert!(vertex_field_names.contains(&"@collection_name".to_string()));
        Ok(())
    };
    let vertices_result = graph_loader.do_vertices(handle_vertices).await;
    assert!(vertices_result.is_ok());

    let handle_edges = move |from_ids: &Vec<Vec<u8>>,
                             to_ids: &Vec<Vec<u8>>,
                             columns: &mut Vec<Vec<Value>>,
                             edge_field_names: &Vec<String>| {
        assert_eq!(from_ids.len(), 9);
        assert_eq!(from_ids.len(), to_ids.len());
        assert_eq!(columns.len(), 9);

        for (e_index, from_id) in from_ids.iter().enumerate() {
            let from_id_str = from_id.iter().map(|x| *x as char).collect::<String>();
            let to_id_str = to_ids[e_index]
                .iter()
                .map(|x| *x as char)
                .collect::<String>();
            assert_eq!(from_id_str, format!("{}/{}", VERTEX_COLLECTION, e_index));
            assert_eq!(to_id_str, format!("{}/{}", VERTEX_COLLECTION, e_index + 1));
        }

        for (e_index, to_id) in to_ids.iter().enumerate() {
            let from_id_str = from_ids[e_index]
                .iter()
                .map(|x| *x as char)
                .collect::<String>();
            let to_id_str = to_id.iter().map(|x| *x as char).collect::<String>();
            assert_eq!(from_id_str, format!("{}/{}", VERTEX_COLLECTION, e_index));
            assert_eq!(to_id_str, format!("{}/{}", VERTEX_COLLECTION, e_index + 1));
        }

        for edge in columns.iter() {
            assert_eq!(edge.len(), 1);
            assert_eq!(edge_field_names.len(), 1);

            let collection_name = &edge
                [get_attribute_position_from_fields(edge_field_names, "@collection_name")]
            .as_str()
            .unwrap();
            let expected_collection_name = EDGE_COLLECTION;
            assert_eq!(collection_name, &expected_collection_name);
        }

        assert_eq!(edge_field_names.len(), 1);
        assert!(edge_field_names.contains(&"@collection_name".to_string()));
        Ok(())
    };
    let edges_result = graph_loader.do_edges(handle_edges).await;
    assert!(edges_result.is_ok());

    teardown().await;
}

#[tokio::test]
#[serial]
async fn init_named_graph_loader_with_data_all_v_and_e_attributes_all_by_boolean() {
    setup(true).await;
    let db_config = build_db_config();
    let load_config = build_load_config_with_v_with_e(true, true);
    let global_fields = vec![];
    let graph_loader_res = GraphLoader::new_named(
        db_config,
        load_config,
        GRAPH.to_string(),
        Some(global_fields.clone()),
        Some(global_fields),
    )
    .await;
    if let Err(ref e) = graph_loader_res {
        println!("{:?}", e);
    }

    assert!(graph_loader_res.is_ok());

    // check that data is loaded.
    // in this case, no global vertex attributes have been requested
    // therefore only the _key attribute is loaded for vertex documents
    let graph_loader = graph_loader_res.unwrap();
    let handle_vertices = move |vertex_ids: &Vec<Vec<u8>>,
                                columns: &mut Vec<Vec<Value>>,
                                vertex_field_names: &Vec<String>| {
        assert_eq!(vertex_ids.len(), 10);
        assert_eq!(columns.len(), 10);

        for (v_index, v_id) in vertex_ids.iter().enumerate() {
            let id = v_id.iter().map(|x| *x as char).collect::<String>();
            let expected_id = format!("{}/{}", VERTEX_COLLECTION, v_index);
            assert_eq!(id, expected_id);
        }

        for (v_index, vertex_json_arr) in columns.iter().enumerate() {
            assert_eq!(vertex_json_arr.len(), 1);
            let vertex = &vertex_json_arr[0];
            // x, y, y including _key and _rev
            assert_eq!(5, vertex.as_object().unwrap().len());

            let x = &vertex
                .as_object()
                .unwrap()
                .get("x")
                .unwrap()
                .as_u64()
                .unwrap();
            let y = &vertex
                .as_object()
                .unwrap()
                .get("y")
                .unwrap()
                .as_u64()
                .unwrap();
            let z = &vertex
                .as_object()
                .unwrap()
                .get("z")
                .unwrap()
                .as_u64()
                .unwrap();
            let expected_x_value = (v_index + 1) as u64;
            let expected_y_value = (v_index + 2) as u64;
            let expected_z_value = (v_index + 3) as u64;
            assert_eq!(x, &expected_x_value);
            assert_eq!(y, &expected_y_value);
            assert_eq!(z, &expected_z_value);
        }

        assert_eq!(vertex_field_names.len(), 0);
        Ok(())
    };
    let vertices_result = graph_loader.do_vertices(handle_vertices).await;
    assert!(vertices_result.is_ok());

    let handle_edges = move |from_ids: &Vec<Vec<u8>>,
                             to_ids: &Vec<Vec<u8>>,
                             columns: &mut Vec<Vec<Value>>,
                             edge_field_names: &Vec<String>| {
        assert_eq!(from_ids.len(), 9);
        assert_eq!(from_ids.len(), to_ids.len());
        assert_eq!(columns.len(), 9);

        for (e_index, from_id) in from_ids.iter().enumerate() {
            let from_id_str = from_id.iter().map(|x| *x as char).collect::<String>();
            let to_id_str = to_ids[e_index]
                .iter()
                .map(|x| *x as char)
                .collect::<String>();
            assert_eq!(from_id_str, format!("{}/{}", VERTEX_COLLECTION, e_index));
            assert_eq!(to_id_str, format!("{}/{}", VERTEX_COLLECTION, e_index + 1));
        }

        for (e_index, to_id) in to_ids.iter().enumerate() {
            let from_id_str = from_ids[e_index]
                .iter()
                .map(|x| *x as char)
                .collect::<String>();
            let to_id_str = to_id.iter().map(|x| *x as char).collect::<String>();
            assert_eq!(from_id_str, format!("{}/{}", VERTEX_COLLECTION, e_index));
            assert_eq!(to_id_str, format!("{}/{}", VERTEX_COLLECTION, e_index + 1));
        }

        for (e_index, edge_json_arr) in columns.iter().enumerate() {
            assert_eq!(edge_json_arr.len(), 1);
            assert_eq!(edge_field_names.len(), 0);
            let edge = &edge_json_arr[0];
            assert_eq!(6, edge.as_object().unwrap().len());

            // x, y, z and _id, _key, _rev
            let x = &edge
                .as_object()
                .unwrap()
                .get("x")
                .unwrap()
                .as_u64()
                .unwrap();
            let y = &edge
                .as_object()
                .unwrap()
                .get("y")
                .unwrap()
                .as_u64()
                .unwrap();
            let z = &edge
                .as_object()
                .unwrap()
                .get("z")
                .unwrap()
                .as_u64()
                .unwrap();
            let expected_x_value = (e_index + 1) as u64;
            let expected_y_value = (e_index + 2) as u64;
            let expected_z_value = (e_index + 3) as u64;
            assert_eq!(x, &expected_x_value);
            assert_eq!(y, &expected_y_value);
            assert_eq!(z, &expected_z_value);
        }

        assert_eq!(edge_field_names.len(), 0);
        Ok(())
    };
    let edges_result = graph_loader.do_edges(handle_edges).await;
    assert!(edges_result.is_ok());

    teardown().await;
}

#[tokio::test]
#[serial]
async fn init_unknown_named_graph_loader() {
    let db_config = build_db_config();
    let load_config = build_load_config();

    let graph_loader_res = GraphLoader::new_named(
        db_config,
        load_config,
        "UnknownGraph".to_string(),
        None,
        None,
    )
    .await;

    assert!(graph_loader_res.is_err());
}

#[tokio::test]
#[serial]
async fn init_custom_graph_loader() {
    setup(false).await;
    let db_config = build_db_config();
    let load_config = build_load_config();
    let vertex_collection_info = vec![CollectionInfo {
        name: VERTEX_COLLECTION.to_string(),
        fields: vec![],
    }];
    let edge_collection_info = vec![CollectionInfo {
        name: EDGE_COLLECTION.to_string(),
        fields: vec![],
    }];

    let graph_loader_res = GraphLoader::new_custom(
        db_config,
        load_config,
        vertex_collection_info,
        edge_collection_info,
    )
    .await;

    if let Err(ref e) = graph_loader_res {
        println!("{:?}", e);
    }
    assert!(graph_loader_res.is_ok());
    teardown().await;
}

#[fixture]
fn generate_collection_load_combinations() -> Vec<(bool, bool)> {
    let mut combinations = vec![];
    for &a in &[true, false] {
        for &b in &[true, false] {
            combinations.push((a, b));
        }
    }
    combinations
}

#[tokio::test]
#[serial]
async fn init_custom_graph_loader_with_fields_and_fetch_all_attributes_positive() {
    let test_variants: Vec<(bool, bool)> = generate_collection_load_combinations();
    setup(false).await;

    for (fetch_all_v_attributes, fetch_all_e_attributes) in test_variants {
        let vertex_fields = vec![];
        let edge_fields = vec![];

        let db_config = build_db_config();
        let load_config =
            build_load_config_with_v_with_e(fetch_all_v_attributes, fetch_all_e_attributes);
        let vertex_collection_info = vec![CollectionInfo {
            name: VERTEX_COLLECTION.to_string(),
            fields: vertex_fields,
        }];
        let edge_collection_info = vec![CollectionInfo {
            name: EDGE_COLLECTION.to_string(),
            fields: edge_fields,
        }];

        let graph_loader_res = GraphLoader::new_custom(
            db_config,
            load_config,
            vertex_collection_info,
            edge_collection_info,
        )
        .await;

        if let Err(ref e) = graph_loader_res {
            println!("{:?}", e);
        }
        // in case we never define specific fields (CollectionInfo), it should always pass
        assert!(graph_loader_res.is_ok());
    }
    teardown().await;
}

#[tokio::test]
#[serial]
async fn init_custom_graph_loader_with_fields_and_fetch_all_attributes_negative() {
    setup(false).await;
    let test_variants: Vec<(bool, bool)> = generate_collection_load_combinations();

    for (fetch_all_v_attributes, fetch_all_e_attributes) in test_variants {
        let vertex_fields = vec!["_id".to_string()];
        let edge_fields = vec!["_key".to_string()];

        let db_config = build_db_config();
        let load_config =
            build_load_config_with_v_with_e(fetch_all_v_attributes, fetch_all_e_attributes);
        let vertex_collection_info = vec![CollectionInfo {
            name: VERTEX_COLLECTION.to_string(),
            fields: vertex_fields,
        }];
        let edge_collection_info = vec![CollectionInfo {
            name: EDGE_COLLECTION.to_string(),
            fields: edge_fields,
        }];

        let graph_loader_res = GraphLoader::new_custom(
            db_config,
            load_config,
            vertex_collection_info,
            edge_collection_info,
        )
        .await;

        if let Err(ref e) = graph_loader_res {
            println!("{:?}", e);
        }
        // in case we never define specific fields (CollectionInfo), it should always pass
        if fetch_all_v_attributes || fetch_all_e_attributes {
            assert!(graph_loader_res.is_err());
        } else {
            assert!(graph_loader_res.is_ok());
        }
    }
    teardown().await;
}

#[tokio::test]
#[serial]
async fn init_empty_custom_graph_loader() {
    setup(false).await;
    let is_cluster = is_cluster().await;
    let db_config = build_db_config();
    let load_config = build_load_config();
    let version_str = get_arangodb_version().await.version;
    let (major, minor, _) = extract_version_parts(&version_str).unwrap();

    let graph_loader_res = GraphLoader::new_custom(db_config, load_config, vec![], vec![]).await;

    if let Err(ref e) = graph_loader_res {
        println!("{:?}", e);
    }
    assert!(graph_loader_res.is_ok());

    let graph_loader = graph_loader_res.unwrap();
    let handle_vertices =
        move |_vertex_keys: &Vec<Vec<u8>>,
              _vertex_json: &mut Vec<Vec<Value>>,
              _vertex_field_names: &Vec<String>| { Ok(()) };
    let vertices_result = graph_loader.do_vertices(handle_vertices).await;

    if let Err(ref e) = vertices_result {
        println!("{:?}", e);
    }
    if is_cluster {
        if major > 3 || (major == 3 && minor >= 12) {
            assert!(vertices_result.is_err());
            match vertices_result {
                Err(GraphLoaderError::Other(ref msg))
                    if msg.contains("No vertex collections given!") =>
                {
                    // Expected error
                }
                _ => panic!("Expected GraphLoaderError::Other with 'No vertex collections given!'"),
            }
        }
    } else if major > 3 || (major == 3 && minor >= 12) {
        // single server dump endpoint only supported from 3.12
        // all versions below will fall back to aql.
        // uses dump endpoint, must fail
        assert!(vertices_result.is_err());
    } else {
        // In the SingleServer case we do not have an error as we execute AQL on empty collections.
        // Means we're just not receiving any documents.
        assert!(vertices_result.is_ok());
    }

    let handle_edges = move |_from_ids: &Vec<Vec<u8>>,
                             _to_ids: &Vec<Vec<u8>>,
                             _json: &mut Vec<Vec<Value>>,
                             _fields: &Vec<String>| { Ok(()) };
    let edges_result = graph_loader.do_edges(handle_edges).await;

    if is_cluster {
        if major > 3 || (major == 3 && minor >= 11) {
            assert!(edges_result.is_err());
            match edges_result {
                Err(GraphLoaderError::Other(ref msg))
                    if msg.contains("No edge collections given!") =>
                {
                    // Expected error
                }
                _ => panic!("Expected GraphLoaderError::Other with 'No edge collections given!'"),
            }
        }
    } else if major > 3 || (major == 3 && minor >= 12) {
        // uses dump endpoint, must fail
        assert!(vertices_result.is_err());
    } else {
        // In the SingleServer case we do not have an error as we execute AQL on empty collections.
        // Means we're just not receiving any documents.
        assert!(vertices_result.is_ok());
    }
    if let Err(ref e) = edges_result {
        println!("{:?}", e);
    }

    teardown().await;
}

#[tokio::test]
#[serial]
async fn init_unknown_custom_graph_loader() {
    let db_config = build_db_config();
    let load_config = build_load_config();
    let vertex_collection_info = vec![CollectionInfo {
        name: "UnknownVertex".to_string(),
        fields: vec![],
    }];
    let edge_collection_info = vec![CollectionInfo {
        name: "UnknownEdge".to_string(),
        fields: vec![],
    }];

    let graph_loader_res = GraphLoader::new_custom(
        db_config,
        load_config,
        vertex_collection_info,
        edge_collection_info,
    )
    .await;

    let is_cluster = is_cluster().await;
    if is_cluster {
        // will error out as we cannot compute the shard map during init
        assert!(graph_loader_res.is_err());
    } else {
        // as we are not in a cluster, we can compute the shard map during init
        assert!(graph_loader_res.is_ok());
    }
}

// Helper function to create a binary tree of given depth
async fn create_binary_tree_graph(depth: usize) {
    let db_config = build_db_config();
    let client = build_test_client();

    // Drop graph if exists
    let drop_url = format!("{}/_api/gharial/{}", db_config.endpoints[0], GRAPH);
    let _ = client
        .delete(&drop_url)
        .basic_auth(USERNAME, Some(PASSWORD))
        .query(&[("dropCollections", "true")])
        .send()
        .await;

    // Create graph
    let create_graph_body = json!({
        "name": GRAPH,
        "edgeDefinitions": [{
            "collection": EDGE_COLLECTION,
            "from": [VERTEX_COLLECTION],
            "to": [VERTEX_COLLECTION]
        }],
        "orphanCollections": []
    });

    let create_url = format!("{}/_api/gharial", db_config.endpoints[0]);
    let resp = client
        .post(&create_url)
        .basic_auth(USERNAME, Some(PASSWORD))
        .header("Content-Type", "application/json")
        .body(serde_json::to_string(&create_graph_body).unwrap())
        .send()
        .await
        .unwrap();

    assert!(
        resp.status().is_success(),
        "Failed to create graph: status={}",
        resp.status()
    );

    // Calculate total number of vertices in a complete binary tree
    // Formula: 2^(depth+1) - 1
    let total_vertices = (1 << (depth + 1)) - 1;

    // Insert vertices with depth attribute
    let vertex_url = format!(
        "{}/_api/document/{}",
        db_config.endpoints[0], VERTEX_COLLECTION
    );

    for i in 0..total_vertices {
        // Calculate depth: floor(log2(i+1))
        let vertex_depth = if i == 0 {
            0
        } else {
            ((i + 1) as u32).ilog2() as u64
        };

        let doc = json!({
            "_key": i.to_string(),
            "depth": vertex_depth
        });
        client
            .post(&vertex_url)
            .basic_auth(USERNAME, Some(PASSWORD))
            .header("Content-Type", "application/json")
            .body(serde_json::to_string(&doc).unwrap())
            .send()
            .await
            .unwrap();
    }

    // Insert edges
    let edge_url = format!(
        "{}/_api/document/{}",
        db_config.endpoints[0], EDGE_COLLECTION
    );

    let mut edge_key = 0;
    for i in 0..total_vertices {
        let left_child = 2 * i + 1;
        let right_child = 2 * i + 2;

        // Add left child edge if it exists
        if left_child < total_vertices {
            let child_depth = ((left_child + 1) as u32).ilog2() as u64;
            let doc = json!({
                "_key": edge_key.to_string(),
                "_from": format!("{}/{}", VERTEX_COLLECTION, i),
                "_to": format!("{}/{}", VERTEX_COLLECTION, left_child),
                "depth": child_depth,
                "type": "left"
            });
            client
                .post(&edge_url)
                .basic_auth(USERNAME, Some(PASSWORD))
                .header("Content-Type", "application/json")
                .body(serde_json::to_string(&doc).unwrap())
                .send()
                .await
                .unwrap();
            edge_key += 1;
        }

        // Add right child edge if it exists
        if right_child < total_vertices {
            let child_depth = ((right_child + 1) as u32).ilog2() as u64;
            let doc = json!({
                "_key": edge_key.to_string(),
                "_from": format!("{}/{}", VERTEX_COLLECTION, i),
                "_to": format!("{}/{}", VERTEX_COLLECTION, right_child),
                "depth": child_depth,
                "type": "right"
            });
            client
                .post(&edge_url)
                .basic_auth(USERNAME, Some(PASSWORD))
                .header("Content-Type", "application/json")
                .body(serde_json::to_string(&doc).unwrap())
                .send()
                .await
                .unwrap();
            edge_key += 1;
        }
    }
}

#[tokio::test]
#[serial]
async fn test_aql_graph_loader_full_topology() {
    // Create binary tree of depth 10
    create_binary_tree_graph(10).await;

    let db_config = build_db_config();
    let batch_size = 100;

    // Expected counts
    let total_vertices = (1 << 11) - 1; // 2047 vertices
    let total_edges = total_vertices - 1; // 2046 edges

    // Build AQL queries to load full topology
    let vertex_query = AqlQuery::new(
        format!(
            "FOR v IN {} RETURN {{vertices: [{{_id: v._id}}]}}",
            VERTEX_COLLECTION
        ),
        HashMap::new(),
    );

    let edge_query = AqlQuery::new(
        format!(
            "FOR e IN {} RETURN {{edges: [{{_from: e._from, _to: e._to}}]}}",
            EDGE_COLLECTION
        ),
        HashMap::new(),
    );

    // Create loader with no attributes
    let loader = AqlGraphLoader::new(
        db_config,
        batch_size,
        vec![], // No vertex attributes
        vec![], // No edge attributes
        vec![vec![vertex_query], vec![edge_query]],
    )
    .await
    .unwrap();

    // Track what we've received
    let received_vertices = Arc::new(Mutex::new(HashSet::new()));
    let received_edges = Arc::new(Mutex::new(HashSet::new()));
    let batch_info = Arc::new(Mutex::new(Vec::new()));
    let vertices_done = Arc::new(Mutex::new(false));

    let received_vertices_clone = received_vertices.clone();
    let received_edges_clone = received_edges.clone();
    let batch_info_clone = batch_info.clone();
    let vertices_done_clone = vertices_done.clone();

    // Load the graph
    loader
        .do_load(move |batch| {
            let mut batch_info = batch_info_clone.lock().unwrap();
            let mut received_v = received_vertices_clone.lock().unwrap();
            let mut received_e = received_edges_clone.lock().unwrap();
            let mut v_done = vertices_done_clone.lock().unwrap();

            // Record batch information
            let batch_desc = (batch.vertex_ids.len(), batch.edge_from_ids.len());
            batch_info.push(batch_desc);

            // Collect vertices
            for v_id in &batch.vertex_ids {
                let id_str = String::from_utf8(v_id.clone()).unwrap();
                received_v.insert(id_str);

                // If we're seeing edges, vertices should be done
                if !batch.edge_from_ids.is_empty() {
                    *v_done = true;
                }
            }

            // Collect edges
            for (from_id, to_id) in batch.edge_from_ids.iter().zip(batch.edge_to_ids.iter()) {
                let from_str = String::from_utf8(from_id.clone()).unwrap();
                let to_str = String::from_utf8(to_id.clone()).unwrap();
                let edge_str = format!("{}-->{}", from_str, to_str);
                received_e.insert(edge_str);
            }

            // Verify no attributes - when empty, vectors should be truly empty to save memory
            assert_eq!(
                batch.vertex_attributes.len(),
                0,
                "Expected empty vertex_attributes vector when no attributes requested"
            );

            assert_eq!(
                batch.edge_attributes.len(),
                0,
                "Expected empty edge_attributes vector when no attributes requested"
            );

            Ok(())
        })
        .await
        .unwrap();

    // Verify results (scope the locks to drop before await)
    {
        let received_v = received_vertices.lock().unwrap();
        let received_e = received_edges.lock().unwrap();
        let batch_info = batch_info.lock().unwrap();
        let v_done = vertices_done.lock().unwrap();

        // Check counts
        assert_eq!(
            received_v.len(),
            total_vertices,
            "Expected {} vertices, got {}",
            total_vertices,
            received_v.len()
        );
        assert_eq!(
            received_e.len(),
            total_edges,
            "Expected {} edges, got {}",
            total_edges,
            received_e.len()
        );

        // Verify batching occurred (with batch_size=100, we should have multiple batches)
        assert!(
            batch_info.len() > 1,
            "Expected multiple batches, got {}",
            batch_info.len()
        );

        // Verify vertices came before edges (once edges start, no more vertices)
        assert!(!*v_done, "Expected all vertices before edges");
    }

    teardown().await;
}

#[tokio::test]
#[serial]
async fn test_aql_graph_loader_filtered_with_depth() {
    // Create binary tree of depth 10
    create_binary_tree_graph(10).await;

    let db_config = build_db_config();
    let batch_size = 100;

    // Expected counts for depth <= 5
    // Vertices: sum from depth 0 to 5 = 2^6 - 1 = 63 vertices
    let expected_vertices = 63;
    // Edges: 62 (all except root has parent)
    let expected_edges = 62;

    // Build AQL queries with depth filter
    let vertex_query = AqlQuery::new(
        format!(
            "FOR v IN {} FILTER v.depth <= 5 RETURN {{vertices: [{{_id: v._id, depth: v.depth}}]}}",
            VERTEX_COLLECTION
        ),
        HashMap::new(),
    );

    let edge_query = AqlQuery::new(
        format!(
            "FOR e IN {} FILTER e.depth <= 5 RETURN {{edges: [{{_from: e._from, _to: e._to, depth: e.depth}}]}}",
            EDGE_COLLECTION
        ),
        HashMap::new(),
    );

    // Create loader with depth attribute
    let loader = AqlGraphLoader::new(
        db_config,
        batch_size,
        vec![DataItem::new("depth".to_string(), DataType::U64)],
        vec![DataItem::new("depth".to_string(), DataType::U64)],
        vec![vec![vertex_query], vec![edge_query]],
    )
    .await
    .unwrap();

    // Track what we've received
    let received_vertices = Arc::new(Mutex::new(Vec::new()));
    let received_edges = Arc::new(Mutex::new(Vec::new()));
    let batch_count = Arc::new(Mutex::new(0));

    let received_vertices_clone = received_vertices.clone();
    let received_edges_clone = received_edges.clone();
    let batch_count_clone = batch_count.clone();

    // Load the graph
    loader
        .do_load(move |batch| {
            let mut batch_count = batch_count_clone.lock().unwrap();
            *batch_count += 1;

            let mut received_v = received_vertices_clone.lock().unwrap();
            let mut received_e = received_edges_clone.lock().unwrap();

            // Collect vertices with depth
            for (v_id, attrs) in batch.vertex_ids.iter().zip(batch.vertex_attributes.iter()) {
                let id_str = String::from_utf8(v_id.clone()).unwrap();
                assert_eq!(attrs.len(), 1, "Expected 1 vertex attribute");
                let depth = attrs[0].as_u64().unwrap();
                assert!(depth <= 5, "Vertex depth {} exceeds filter", depth);
                received_v.push((id_str, depth));
            }

            // Collect edges with depth
            for ((from_id, to_id), attrs) in batch
                .edge_from_ids
                .iter()
                .zip(batch.edge_to_ids.iter())
                .zip(batch.edge_attributes.iter())
            {
                let from_str = String::from_utf8(from_id.clone()).unwrap();
                let to_str = String::from_utf8(to_id.clone()).unwrap();
                assert_eq!(attrs.len(), 1, "Expected 1 edge attribute");
                let edge_depth = attrs[0].as_u64().unwrap();
                assert!(edge_depth <= 5, "Edge depth {} exceeds filter", edge_depth);

                // Verify edge depth matches target vertex depth
                // Extract target vertex key
                let to_key = to_str
                    .split('/')
                    .next_back()
                    .unwrap()
                    .parse::<usize>()
                    .unwrap();
                let expected_depth = if to_key == 0 {
                    0
                } else {
                    (to_key + 1).ilog2() as u64
                };
                assert_eq!(
                    edge_depth, expected_depth,
                    "Edge depth {} doesn't match target vertex depth {}",
                    edge_depth, expected_depth
                );

                received_e.push((from_str, to_str, edge_depth));
            }

            Ok(())
        })
        .await
        .unwrap();

    // Verify results (scope the locks to drop before await)
    {
        let received_v = received_vertices.lock().unwrap();
        let received_e = received_edges.lock().unwrap();
        let batch_count = batch_count.lock().unwrap();

        // Check counts
        assert_eq!(
            received_v.len(),
            expected_vertices,
            "Expected {} vertices, got {}",
            expected_vertices,
            received_v.len()
        );
        assert_eq!(
            received_e.len(),
            expected_edges,
            "Expected {} edges, got {}",
            expected_edges,
            received_e.len()
        );

        // With 63 items, batch size 100, should fit in a single batch (or very few)
        assert!(
            *batch_count <= 2,
            "Expected single batch or very few, got {}",
            *batch_count
        );

        // Verify all vertex depths are <= 5
        for (_, depth) in received_v.iter() {
            assert!(*depth <= 5);
        }

        // Verify all edge depths are <= 5
        for (_, _, depth) in received_e.iter() {
            assert!(*depth <= 5);
        }
    }

    teardown().await;
}

#[tokio::test]
#[serial]
async fn test_aql_graph_loader_left_edges_only() {
    // Create binary tree of depth 10
    create_binary_tree_graph(10).await;

    let db_config = build_db_config();
    let batch_size = 100;

    // Expected counts
    let total_vertices = (1 << 11) - 1; // 2047 vertices
    // For left edges only: each level except leaves has left children
    // In a complete binary tree of depth 10, we have 11 levels (0-10)
    // Each level i (0 <= i < 10) has 2^i vertices, each with a left child
    // Total left edges: sum of 2^i for i=0 to 9 = 2^10 - 1 = 1023
    let expected_left_edges = 1023;

    // Build AQL queries
    let vertex_query = AqlQuery::new(
        format!(
            "FOR v IN {} RETURN {{vertices: [{{_id: v._id, depth: v.depth}}]}}",
            VERTEX_COLLECTION
        ),
        HashMap::new(),
    );

    let edge_query = AqlQuery::new(
        format!(
            "FOR e IN {} FILTER e.type == 'left' RETURN {{edges: [{{_from: e._from, _to: e._to, depth: e.depth}}]}}",
            EDGE_COLLECTION
        ),
        HashMap::new(),
    );

    // Create loader with depth attribute (not type)
    let loader = AqlGraphLoader::new(
        db_config,
        batch_size,
        vec![DataItem::new("depth".to_string(), DataType::U64)],
        vec![DataItem::new("depth".to_string(), DataType::U64)],
        vec![vec![vertex_query, edge_query]],
    )
    .await
    .unwrap();

    // Track what we've received
    let received_vertices = Arc::new(Mutex::new(HashMap::new()));
    let received_edges = Arc::new(Mutex::new(Vec::new()));

    let received_vertices_clone = received_vertices.clone();
    let received_edges_clone = received_edges.clone();

    // Load the graph
    loader
        .do_load(move |batch| {
            let mut received_v = received_vertices_clone.lock().unwrap();
            let mut received_e = received_edges_clone.lock().unwrap();

            // Collect vertices with depth
            for (v_id, attrs) in batch.vertex_ids.iter().zip(batch.vertex_attributes.iter()) {
                let id_str = String::from_utf8(v_id.clone()).unwrap();
                assert_eq!(attrs.len(), 1, "Expected 1 vertex attribute");
                let depth = attrs[0].as_u64().unwrap();
                received_v.insert(id_str, depth);
            }

            // Collect edges with depth (type attribute not fetched)
            for ((from_id, to_id), attrs) in batch
                .edge_from_ids
                .iter()
                .zip(batch.edge_to_ids.iter())
                .zip(batch.edge_attributes.iter())
            {
                let from_str = String::from_utf8(from_id.clone()).unwrap();
                let to_str = String::from_utf8(to_id.clone()).unwrap();
                assert_eq!(attrs.len(), 1, "Expected 1 edge attribute (depth only)");
                let edge_depth = attrs[0].as_u64().unwrap();
                received_e.push((from_str.clone(), to_str.clone(), edge_depth));
            }

            Ok(())
        })
        .await
        .unwrap();

    // Verify results (scope the locks to drop before await)
    {
        let received_v = received_vertices.lock().unwrap();
        let received_e = received_edges.lock().unwrap();

        // Check counts
        assert_eq!(
            received_v.len(),
            total_vertices,
            "Expected {} vertices, got {}",
            total_vertices,
            received_v.len()
        );
        assert_eq!(
            received_e.len(),
            expected_left_edges,
            "Expected {} left edges, got {}",
            expected_left_edges,
            received_e.len()
        );

        // Verify structure: each edge should be from parent to left child
        for (from_str, to_str, edge_depth) in received_e.iter() {
            let from_key = from_str
                .split('/')
                .next_back()
                .unwrap()
                .parse::<usize>()
                .unwrap();
            let to_key = to_str
                .split('/')
                .next_back()
                .unwrap()
                .parse::<usize>()
                .unwrap();

            // Left child relationship: to_key = 2 * from_key + 1
            assert_eq!(
                to_key,
                2 * from_key + 1,
                "Edge {}-->{} is not a left child edge",
                from_str,
                to_str
            );

            // Verify edge depth matches target vertex depth
            let target_depth = received_v.get(to_str).unwrap();
            assert_eq!(
                edge_depth, target_depth,
                "Edge depth {} doesn't match target vertex depth {}",
                edge_depth, target_depth
            );
        }
    }

    teardown().await;
}
