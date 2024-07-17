use arangors::graph::{EdgeDefinition, Graph};
use arangors::Connection;
use lightning::{
    CollectionInfo, DataLoadConfiguration, DataLoadConfigurationBuilder, DatabaseConfiguration,
    DatabaseConfigurationBuilder, GraphLoader,
};
use serial_test::serial;

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
    let graph_exist = db.graph(GRAPH).await;
    if graph_exist.is_err() {
        println!("{:?}", graph_exist);
        let graph_res = db.create_graph(graph, false).await;
        print!("{:?}", graph_res);
        assert!(graph_res.is_ok());
    }
}

async fn drop_graph() {
    let conn = Connection::establish_basic_auth(DATABASE_URL, USERNAME, PASSWORD)
        .await
        .unwrap();

    let db = conn.db(DATABASE).await.unwrap();
    let graph_exist = db.graph(GRAPH).await;
    if let Ok(_) = graph_exist {
        let drop_res = db.drop_graph(GRAPH, true).await;
        assert!(drop_res.is_ok());
    }
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
    setup().await;
    let db_config = build_db_config();
    let load_config = build_load_config();

    let graph_loader_res =
        GraphLoader::new_named(db_config, load_config, "UnknownGraph".to_string(), None).await;

    assert!(graph_loader_res.is_err());
    teardown().await;
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
async fn init_unknown_custom_graph_loader() {
    setup().await;
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

    assert!(graph_loader_res.is_err());
    teardown().await;
}
