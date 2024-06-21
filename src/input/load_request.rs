use std::time::SystemTime;

pub type CollectionAttribute = String;
pub type CollectionName = String;

#[derive(Clone)]
pub struct CollectionDescription {
    pub name: CollectionName,
    pub fields: Vec<CollectionAttribute>,
}

pub fn generate_collection_list(
    collections: Vec<CollectionDescription>,
) -> Vec<String> {
    let mut list = Vec::new();
    for collection in collections {
        list.push(collection.name.clone());
    }
    list
}

pub struct DataLoadRequest {
    pub database: String,
    pub graph_name: Option<String>,
    pub vertex_collections: Vec<CollectionDescription>,
    pub vertex_attributes: Vec<CollectionAttribute>,
    pub edge_collections: Vec<CollectionDescription>,
    pub configuration: DataLoadConfiguration,
}

pub struct DataLoadConfiguration {
    pub database_config: DatabaseConfiguration,
    pub parallelism: Option<u32>,
    pub batch_size: Option<u64>,
}

pub struct Progress {
    pub current: u32, // current step
    pub message: String, // message
    pub total: u32, // total amount of steps
    pub timings: Vec<SystemTime>, // timings
}

impl Progress {
    pub fn new(total: u32) -> Progress {
        let mut timings: Vec<SystemTime> = Vec::with_capacity(total as usize);
        timings.push(std::time::SystemTime::now());
        Progress {
            current: 0,
            message: "Initializing...".to_string(),
            total,
            timings,
        }
    }

    pub fn set_current_state(&mut self, message: String) -> Result<(), String> {
        self.current += 1;
        if self.current > self.total {
            return Err(
                format!("Current step {} cannot be greater than total steps {}", self.current, self.total)
            );
        }
        self.message = message;
        self.timings.push(std::time::SystemTime::now());
        Ok(())
    }

    pub fn get_start_time(&self) -> SystemTime {
        self.timings[0]
    }

    pub fn is_finished(&self) -> bool {
        self.current == self.total
    }
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
