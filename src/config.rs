#[derive(Clone, Debug)]
pub struct DatabaseConfiguration {
    pub database: String,
    pub endpoints: Vec<String>,
    pub username: String,
    pub password: String,
    pub jwt_token: String,
    pub tls_cert: Option<String>,
}

impl Default for DatabaseConfiguration {
    fn default() -> Self {
        DatabaseConfigurationBuilder::new().build()
    }
}

pub struct DatabaseConfigurationBuilder {
    database: Option<String>,
    endpoints: Option<Vec<String>>,
    username: Option<String>,
    password: Option<String>,
    jwt_token: Option<String>,
    tls_cert: Option<String>,
}

impl Default for DatabaseConfigurationBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl DatabaseConfigurationBuilder {
    pub fn new() -> Self {
        DatabaseConfigurationBuilder {
            database: None,
            endpoints: None,
            username: None,
            password: None,
            jwt_token: None,
            tls_cert: None,
        }
    }

    pub fn database(mut self, database: String) -> Self {
        self.database = Some(database);
        self
    }

    pub fn endpoints(mut self, endpoints: Vec<String>) -> Self {
        self.endpoints = Some(endpoints);
        self
    }

    pub fn username(mut self, username: String) -> Self {
        self.username = Some(username);
        self
    }

    pub fn password(mut self, password: String) -> Self {
        self.password = Some(password);
        self
    }

    pub fn jwt_token(mut self, jwt_token: String) -> Self {
        self.jwt_token = Some(jwt_token);
        self
    }

    pub fn tls_cert(mut self, tls_cert: String) -> Self {
        self.tls_cert = Some(tls_cert);
        self
    }

    pub fn build(self) -> DatabaseConfiguration {
        DatabaseConfiguration {
            database: self.database.unwrap_or_else(|| "_system".to_string()),
            endpoints: self
                .endpoints
                .unwrap_or_else(|| vec!["http://localhost:8529".to_string()]),
            username: self.username.unwrap_or_else(|| "root".to_string()),
            password: self.password.unwrap_or_default(),
            jwt_token: self.jwt_token.unwrap_or_default(),
            tls_cert: self.tls_cert,
        }
    }
}

#[derive(Clone, Debug)]
pub struct DataLoadConfiguration {
    pub parallelism: u32,
    pub batch_size: u64,
    pub prefetch_count: u32,
    pub load_all_vertex_attributes: bool,
    pub load_all_edge_attributes: bool,
}

impl Default for DataLoadConfiguration {
    fn default() -> Self {
        DataLoadConfigurationBuilder::new().build()
    }
}

impl DataLoadConfiguration {
    pub fn new(
        parallelism: Option<u32>,
        batch_size: Option<u64>,
        prefetch_count: Option<u32>,
        load_all_vertex_attributes: bool,
        load_all_edge_attributes: bool,
    ) -> Self {
        DataLoadConfiguration {
            parallelism: parallelism.unwrap_or(8),
            batch_size: batch_size.unwrap_or(100_000),
            prefetch_count: prefetch_count.unwrap_or(5),
            load_all_vertex_attributes,
            load_all_edge_attributes,
        }
    }
}

pub struct DataLoadConfigurationBuilder {
    parallelism: Option<u32>,
    batch_size: Option<u64>,
    prefetch_count: Option<u32>,
    load_all_vertex_attributes: bool,
    load_all_edge_attributes: bool,
}

impl Default for DataLoadConfigurationBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl DataLoadConfigurationBuilder {
    pub fn new() -> Self {
        DataLoadConfigurationBuilder {
            parallelism: None,
            batch_size: None,
            prefetch_count: None,
            load_all_vertex_attributes: false,
            load_all_edge_attributes: false,
        }
    }

    pub fn parallelism(mut self, parallelism: u32) -> Self {
        self.parallelism = Some(parallelism);
        self
    }

    pub fn batch_size(mut self, batch_size: u64) -> Self {
        self.batch_size = Some(batch_size);
        self
    }

    pub fn prefetch_count(mut self, prefetch_count: u32) -> Self {
        self.prefetch_count = Some(prefetch_count);
        self
    }

    pub fn load_all_vertex_attributes(mut self, load_all_vertex_attributes: bool) -> Self {
        self.load_all_vertex_attributes = load_all_vertex_attributes;
        self
    }

    pub fn load_all_edge_attributes(mut self, load_all_edge_attributes: bool) -> Self {
        self.load_all_edge_attributes = load_all_edge_attributes;
        self
    }

    pub fn build(self) -> DataLoadConfiguration {
        DataLoadConfiguration::new(
            self.parallelism,
            self.batch_size,
            self.prefetch_count,
            self.load_all_vertex_attributes,
            self.load_all_edge_attributes,
        )
    }
}

/// Configuration for custom AQL queries
/// 
/// Supports three modes:
/// 1. Separate vertex and edge queries (Option A)
/// 2. Combined query that returns both vertices and edges (Option B)
/// 
/// For combined queries, the result must include a `_type` field indicating
/// whether the document is a vertex or edge: `"vertex"` or `"edge"`.
#[derive(Clone, Debug)]
pub struct CustomAqlQueries {
    /// Optional: Custom AQL query for vertices
    /// Query must return documents with `_id` field for vertices
    pub vertex_query: Option<String>,
    /// Optional: Custom AQL query for edges  
    /// Query must return documents with `_from` and `_to` fields for edges
    pub edge_query: Option<String>,
    /// Optional: Combined query that returns both vertices and edges
    /// Results must include a `_type` field: "vertex" or "edge"
    /// Vertices must have `_id`, edges must have `_from` and `_to`
    pub combined_query: Option<String>,
    /// Optional bind variables for the queries (shared across all queries if provided)
    pub bind_vars: Option<std::collections::HashMap<String, serde_json::Value>>,
}

impl CustomAqlQueries {
    /// Create custom queries with separate vertex and edge queries
    pub fn new_separate(
        vertex_query: String,
        edge_query: String,
        bind_vars: Option<std::collections::HashMap<String, serde_json::Value>>,
    ) -> Self {
        Self {
            vertex_query: Some(vertex_query),
            edge_query: Some(edge_query),
            combined_query: None,
            bind_vars,
        }
    }

    /// Create a combined query that returns both vertices and edges
    pub fn new_combined(
        combined_query: String,
        bind_vars: Option<std::collections::HashMap<String, serde_json::Value>>,
    ) -> Self {
        Self {
            vertex_query: None,
            edge_query: None,
            combined_query: Some(combined_query),
            bind_vars,
        }
    }

    /// Validate that the configuration is valid
    pub fn validate(&self) -> Result<(), String> {
        let has_separate = self.vertex_query.is_some() && self.edge_query.is_some();
        let has_combined = self.combined_query.is_some();
        
        if !has_separate && !has_combined {
            return Err("Either separate queries (vertex_query + edge_query) or combined_query must be provided".to_string());
        }
        
        if has_separate && has_combined {
            return Err("Cannot use both separate queries and combined query at the same time".to_string());
        }
        
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn test_custom_aql_queries_new_separate() {
        let queries = CustomAqlQueries::new_separate(
            "FOR v IN vertices RETURN v".to_string(),
            "FOR e IN edges RETURN e".to_string(),
            None,
        );
        
        assert!(queries.vertex_query.is_some());
        assert!(queries.edge_query.is_some());
        assert!(queries.combined_query.is_none());
        assert!(queries.validate().is_ok());
    }

    #[test]
    fn test_custom_aql_queries_new_combined() {
        let queries = CustomAqlQueries::new_combined(
            "FOR doc IN vertices RETURN MERGE(doc, {_type: 'vertex'})".to_string(),
            None,
        );
        
        assert!(queries.vertex_query.is_none());
        assert!(queries.edge_query.is_none());
        assert!(queries.combined_query.is_some());
        assert!(queries.validate().is_ok());
    }

    #[test]
    fn test_custom_aql_queries_validate_separate_success() {
        let queries = CustomAqlQueries::new_separate(
            "FOR v IN vertices RETURN v".to_string(),
            "FOR e IN edges RETURN e".to_string(),
            None,
        );
        assert!(queries.validate().is_ok());
    }

    #[test]
    fn test_custom_aql_queries_validate_combined_success() {
        let queries = CustomAqlQueries::new_combined(
            "FOR doc IN vertices RETURN MERGE(doc, {_type: 'vertex'})".to_string(),
            None,
        );
        assert!(queries.validate().is_ok());
    }

    #[test]
    fn test_custom_aql_queries_validate_empty_fails() {
        let queries = CustomAqlQueries {
            vertex_query: None,
            edge_query: None,
            combined_query: None,
            bind_vars: None,
        };
        assert!(queries.validate().is_err());
        let err = queries.validate().unwrap_err();
        assert!(err.contains("Either separate queries"));
    }

    #[test]
    fn test_custom_aql_queries_validate_both_fails() {
        let mut queries = CustomAqlQueries::new_separate(
            "FOR v IN vertices RETURN v".to_string(),
            "FOR e IN edges RETURN e".to_string(),
            None,
        );
        queries.combined_query = Some("FOR doc IN vertices RETURN doc".to_string());
        assert!(queries.validate().is_err());
        let err = queries.validate().unwrap_err();
        assert!(err.contains("Cannot use both separate queries and combined query"));
    }

    #[test]
    fn test_custom_aql_queries_validate_partial_separate_fails() {
        let queries = CustomAqlQueries {
            vertex_query: Some("FOR v IN vertices RETURN v".to_string()),
            edge_query: None,
            combined_query: None,
            bind_vars: None,
        };
        assert!(queries.validate().is_err());
    }

    #[test]
    fn test_custom_aql_queries_with_bind_vars() {
        let mut bind_vars = HashMap::new();
        bind_vars.insert("min_age".to_string(), serde_json::Value::Number(18.into()));
        bind_vars.insert("collection".to_string(), serde_json::Value::String("users".to_string()));
        
        let queries = CustomAqlQueries::new_separate(
            "FOR v IN @@collection FILTER v.age >= @min_age RETURN v".to_string(),
            "FOR e IN edges RETURN e".to_string(),
            Some(bind_vars.clone()),
        );
        
        assert!(queries.bind_vars.is_some());
        assert_eq!(queries.bind_vars.as_ref().unwrap().len(), 2);
        assert!(queries.validate().is_ok());
    }
}
