pub struct CollectionDescription {
    pub name: String,
    pub fields: Vec<String>,
}

pub struct DataLoadRequest {
    pub database: String,
    pub vertex_collections: Vec<CollectionDescription>,
    pub edge_collections: Vec<CollectionDescription>,
    pub configuration: DataLoadConfiguration,
}

pub struct DataLoadConfiguration {
    pub database_config: DatabaseConfiguration,
    pub parallelism: Option<u32>,
    pub batch_size: Option<u64>,
}

impl DataLoadConfiguration {
    pub fn default() -> DataLoadConfiguration {
        DataLoadConfiguration {
            database_config: DatabaseConfiguration::default(),
            parallelism: Some(5),
            batch_size: Some(400000),
        }
    }
}

#[derive(Clone)]
pub struct DatabaseConfiguration {
    pub endpoints: Vec<String>,
    // optional components of this configuration
    pub username: Option<String>,
    pub password: Option<String>,
    pub jwt_token: Option<String>,
    pub tls_cert: Option<String>,
}

impl DatabaseConfiguration {
    pub fn default() -> DatabaseConfiguration {
        DatabaseConfiguration {
            endpoints: vec!["http://localhost:8529".into()],
            username: Some("root".into()),
            password: Some("".into()),
            jwt_token: None,
            tls_cert: None,
        }
    }
}
