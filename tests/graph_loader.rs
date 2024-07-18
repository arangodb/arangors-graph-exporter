use arangors::graph::{EdgeDefinition, Graph};
use arangors::Connection;
use lightning::{
    CollectionInfo, DataLoadConfiguration, DataLoadConfigurationBuilder, DatabaseConfiguration,
    DatabaseConfigurationBuilder, GraphLoader,
};
use serial_test::serial;

use arangors::connection::Version;
use lightning::errors::GraphLoaderError;
use rstest::fixture;
use serde_json::Value;
use std::env;

static GRAPH: &str = "IntegrationTestGraph";
static EDGE_COLLECTION: &str = "IntegrationTestEdge";
static VERTEX_COLLECTION: &str = "IntegrationTestVertex";
static USERNAME: &str = "root";
static PASSWORD: &str = "test";
static DATABASE: &str = "_system";
static DATABASE_URL: &str = "http://localhost:8529";

fn get_db_url() -> String {
    env::var("ARANGODB_DB_URL").unwrap_or_else(|_| DATABASE_URL.to_string())
}

fn build_db_config() -> DatabaseConfiguration {
    let endpoints = vec![get_db_url()];
    let db_config_builder = DatabaseConfigurationBuilder::new()
        .endpoints(endpoints)
        .username(USERNAME.to_string())
        .password(PASSWORD.to_string())
        .database(DATABASE.to_string());
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
    let conn = Connection::establish_basic_auth(DATABASE_URL, USERNAME, PASSWORD)
        .await
        .unwrap();
    let info = conn.into_admin().await;
    let role = info.unwrap().server_role().await.unwrap();
    if role == "COORDINATOR" {
        return true;
    }
    false
}

async fn get_arangodb_version() -> Version {
    let conn = Connection::establish_basic_auth(DATABASE_URL, USERNAME, PASSWORD)
        .await
        .unwrap();
    let db = conn.db(DATABASE).await.unwrap();
    db.arango_version().await.unwrap()
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

async fn create_graph() {
    let conn = Connection::establish_basic_auth(DATABASE_URL, USERNAME, PASSWORD)
        .await
        .unwrap();

    let edge_definition = EdgeDefinition {
        collection: EDGE_COLLECTION.to_string(),
        from: vec![VERTEX_COLLECTION.to_string()],
        to: vec![VERTEX_COLLECTION.to_string()],
    };
    let graph = Graph::builder()
        .name(GRAPH.to_string())
        .edge_definitions(vec![edge_definition])
        .orphan_collections(vec![])
        .build();

    let db = conn.db(DATABASE).await.unwrap();
    let _ = db.drop_graph(GRAPH, true).await;
    let graph_res = db.create_graph(graph, false).await;
    assert!(graph_res.is_ok());
}

async fn drop_graph() {
    let conn = Connection::establish_basic_auth(DATABASE_URL, USERNAME, PASSWORD)
        .await
        .unwrap();

    let db = conn.db(DATABASE).await.unwrap();
    let _ = db.drop_graph(GRAPH, true).await;
}

async fn setup() {
    create_graph().await;
}

async fn teardown() {
    drop_graph().await;
}

#[tokio::test]
#[serial]
async fn init_named_graph_loader() {
    setup().await;
    let db_config = build_db_config();
    let load_config = build_load_config();

    let graph_loader_res =
        GraphLoader::new_named(db_config, load_config, GRAPH.to_string(), None).await;
    if let Err(ref e) = graph_loader_res {
        println!("{:?}", e);
    }

    assert!(graph_loader_res.is_ok());
    teardown().await;
}

#[tokio::test]
#[serial]
async fn init_unknown_named_graph_loader() {
    let db_config = build_db_config();
    let load_config = build_load_config();

    let graph_loader_res =
        GraphLoader::new_named(db_config, load_config, "UnknownGraph".to_string(), None).await;

    assert!(graph_loader_res.is_err());
}

#[tokio::test]
#[serial]
async fn init_custom_graph_loader() {
    setup().await;
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
    setup().await;

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
    setup().await;
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
    setup().await;
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

    if is_cluster && (major > 3 || (major == 3 && minor >= 12)) {
        assert!(vertices_result.is_err());
        match vertices_result {
            Err(GraphLoaderError::Other(ref msg)) if msg.contains("No vertex shards found!") => {
                assert!(true)
            }
            _ => assert!(false),
        }
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

    if is_cluster && (major > 3 || (major == 3 && minor >= 12)) {
        assert!(edges_result.is_err());
        match edges_result {
            Err(GraphLoaderError::Other(ref msg)) if msg.contains("No edge shards found!") => {
                assert!(true)
            }
            _ => assert!(false),
        }
    } else {
        // In the SingleServer case we do not have an error as we execute AQL on empty collections.
        // Means we're just not receiving any documents.
        assert!(edges_result.is_ok());
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
