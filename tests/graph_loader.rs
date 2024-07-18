use arangors::graph::{EdgeDefinition, Graph};
use arangors::Connection;
use lightning::{
    CollectionInfo, DataLoadConfiguration, DataLoadConfigurationBuilder, DatabaseConfiguration,
    DatabaseConfigurationBuilder, GraphLoader,
};
use serial_test::serial;

use arangors::connection::Version;
use lightning::errors::GraphLoaderError;
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

#[tokio::test]
#[serial]
async fn init_empty_custom_graph_loader() {
    setup().await;
    let is_cluster = is_cluster().await;
    let db_config = build_db_config();
    let load_config = build_load_config();

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
    assert!(vertices_result.is_err());
    let version_str = get_arangodb_version().await.version;
    let (major, minor, _) = extract_version_parts(&version_str).unwrap();
    if is_cluster && (major > 3 || (major == 3 && minor >= 12)) {
        match vertices_result {
            Err(GraphLoaderError::Other(ref msg)) if msg.contains("No vertex shards found!") => {
                assert!(true)
            }
            _ => assert!(false),
        }
    }

    let handle_edges = move |_from_ids: &Vec<Vec<u8>>,
                             _to_ids: &Vec<Vec<u8>>,
                             _json: &mut Vec<Vec<Value>>,
                             _fields: &Vec<String>| { Ok(()) };
    let edges_result = graph_loader.do_edges(handle_edges).await;
    assert!(edges_result.is_err());
    if is_cluster && (major > 3 || (major == 3 && minor >= 12)) {
        match edges_result {
            Err(GraphLoaderError::Other(ref msg)) if msg.contains("No edge shards found!") => {
                assert!(true)
            }
            _ => assert!(false),
        }
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
