use crate::aql_graph_loader::{AqlGraphLoader, AqlQuery, DataItem};
use crate::config::{DataLoadConfiguration, DatabaseConfiguration};
use crate::errors::GraphLoaderError;
use crate::graph_loader::{CollectionInfo, GraphLoader};

// User-facing functions to load graphs
pub async fn load_named_graph(
    db_config: DatabaseConfiguration,
    load_config: DataLoadConfiguration,
    graph_name: String,
    vertex_global_fields: Option<Vec<String>>,
    edge_global_fields: Option<Vec<String>>,
) -> Result<GraphLoader, GraphLoaderError> {
    GraphLoader::new_named(
        db_config,
        load_config,
        graph_name,
        vertex_global_fields,
        edge_global_fields,
    )
    .await
}

pub async fn load_custom_graph(
    db_config: DatabaseConfiguration,
    load_config: DataLoadConfiguration,
    vertex_collections: Vec<CollectionInfo>,
    edge_collections: Vec<CollectionInfo>,
) -> Result<GraphLoader, GraphLoaderError> {
    GraphLoader::new_custom(db_config, load_config, vertex_collections, edge_collections).await
}

pub fn load_aql_graph(
    db_config: DatabaseConfiguration,
    batch_size: u64,
    vertex_attributes: Vec<DataItem>,
    edge_attributes: Vec<DataItem>,
    queries: Vec<Vec<AqlQuery>>,
) -> Result<AqlGraphLoader, GraphLoaderError> {
    AqlGraphLoader::new(
        db_config,
        batch_size,
        vertex_attributes,
        edge_attributes,
        queries,
    )
}
