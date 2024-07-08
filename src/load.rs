use crate::config::{DataLoadConfiguration, DatabaseConfiguration};
use crate::errors::GraphLoaderError;
use crate::graph_loader::{CollectionInfo, GraphLoader};

// User-facing functions to load graphs
pub async fn load_named_graph(
    db_config: DatabaseConfiguration,
    load_config: DataLoadConfiguration,
    graph_name: String,
) -> Result<GraphLoader, GraphLoaderError> {
    GraphLoader::new_named(db_config, load_config, graph_name).await
}

pub async fn load_custom_graph(
    db_config: DatabaseConfiguration,
    load_config: DataLoadConfiguration,
    vertex_collections: Vec<CollectionInfo>,
    edge_collections: Vec<CollectionInfo>,
) -> Result<GraphLoader, GraphLoaderError> {
    GraphLoader::new_custom(db_config, load_config, vertex_collections, edge_collections).await
}
