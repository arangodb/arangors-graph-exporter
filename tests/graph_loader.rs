use arangors::graph::Graph;
use arangors::Connection;
use lightning::{
    CollectionInfo, DataLoadConfiguration, DataLoadConfigurationBuilder, DatabaseConfiguration,
    DatabaseConfigurationBuilder, GraphLoader,
};

use std::env;

static NAMED_GRAPH: &str = "NamedGraph";
static EDGE_COLLECTION: &str = "EdgeCollection";
static VERTEX_COLLECTION: &str = "VertexCollection";
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

async fn create_named_graph() {
    let conn = Connection::establish_basic_auth(DATABASE_URL, USERNAME, PASSWORD)
        .await
        .unwrap();

    let graph = Graph::builder()
        .name(NAMED_GRAPH.to_string())
        .edge_definitions(vec![])
        .orphan_collections(vec![])
        .build();

    let db = conn.db(DATABASE).await.unwrap();
    let graph_exist = db.graph(NAMED_GRAPH).await;
    if let Err(_) = graph_exist {
        let graph_res = db.create_graph(graph, false).await;
        assert!(graph_res.is_ok());
    }
}

async fn drop_named_graph() {
    let conn = Connection::establish_basic_auth(DATABASE_URL, USERNAME, PASSWORD)
        .await
        .unwrap();

    let db = conn.db(DATABASE).await.unwrap();
    let graph_exist = db.graph(NAMED_GRAPH).await;
    if let Ok(_) = graph_exist {
        let drop_res = db.drop_graph(NAMED_GRAPH, true).await;
        assert!(drop_res.is_ok());
    }
}

async fn setup() {
    create_named_graph().await;
}

async fn teardown() {
    drop_named_graph().await;
}

#[tokio::test]
async fn init_named_graph_loader() {
    setup().await;
    let db_config = build_db_config();
    let load_config = build_load_config();

    let graph_loader_res =
        GraphLoader::new_named(db_config, load_config, NAMED_GRAPH.to_string(), None).await;

    assert!(graph_loader_res.is_ok());
    teardown().await;
}

#[tokio::test]
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

    if let Err(e) = graph_loader_res {
        println!("{:?}", e);
    }
    assert!(graph_loader_res.is_ok());
    teardown().await;
}
