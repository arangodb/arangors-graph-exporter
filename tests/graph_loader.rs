use arangors_graph_exporter::{
    CollectionInfo, DataLoadConfiguration, DataLoadConfigurationBuilder, DatabaseConfiguration,
    DatabaseConfigurationBuilder, GraphLoader,
};
use serial_test::serial;

use arangors_graph_exporter::client::config::ClientConfig;
use arangors_graph_exporter::errors::GraphLoaderError;
use rstest::fixture;
use serde_json::{Value, json};
use std::env;

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
    format!("{}://172.28.0.1:8529", protocol)
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
