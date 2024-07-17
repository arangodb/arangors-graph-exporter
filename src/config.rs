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
            tls_cert: self.tls_cert.map(|cert| cert),
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
        DataLoadConfiguration::new(self.parallelism, self.batch_size, self.prefetch_count, self.load_all_vertex_attributes, self.load_all_edge_attributes)
    }
}
