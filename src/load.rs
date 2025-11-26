use crate::config::{CustomAqlQueries, DataLoadConfiguration, DatabaseConfiguration};
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

/// Load a graph using custom AQL queries
///
/// This allows you to provide custom AQL queries with filtering capabilities.
/// You can either provide separate vertex and edge queries, or a combined query.
///
/// # Example - Separate queries:
/// ```rust,no_run
/// use arangors_graph_exporter::{CustomAqlQueries, DatabaseConfiguration, DataLoadConfiguration, load_with_custom_aql};
///
/// let queries = CustomAqlQueries::new_separate(
///     "FOR v IN vertices FILTER v.age > 18 RETURN v".to_string(),
///     "FOR e IN edges FILTER e.weight > 0.5 RETURN e".to_string(),
///     None,
/// );
/// let loader = load_with_custom_aql(db_config, load_config, queries).await?;
/// ```
///
/// # Example - Combined query:
/// ```rust,no_run
/// use arangors_graph_exporter::{CustomAqlQueries, DatabaseConfiguration, DataLoadConfiguration, load_with_custom_aql};
///
/// let query = r#"
///     FOR doc IN vertices
///         RETURN MERGE(doc, {_type: "vertex"})
///     UNION
///     FOR doc IN edges
///         RETURN MERGE(doc, {_type: "edge"})
/// "#.to_string();
/// let queries = CustomAqlQueries::new_combined(query, None);
/// let loader = load_with_custom_aql(db_config, load_config, queries).await?;
/// ```
pub async fn load_with_custom_aql(
    db_config: DatabaseConfiguration,
    load_config: DataLoadConfiguration,
    custom_queries: CustomAqlQueries,
) -> Result<GraphLoader, GraphLoaderError> {
    GraphLoader::new_with_custom_aql(db_config, load_config, custom_queries).await
}
