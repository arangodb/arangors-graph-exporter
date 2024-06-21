use super::load_request::{
    CollectionDescription, DataLoadConfiguration, DataLoadRequest, DatabaseConfiguration,
};
use pyo3::exceptions::PyValueError;
use pyo3::types::PyDict;
use pyo3::{FromPyObject, PyAny, PyResult};

impl FromPyObject<'_> for DataLoadRequest {
    fn extract(ob: &PyAny) -> PyResult<Self> {
        let input_dict: &PyDict = ob.downcast()?;
        let database_value: &str = input_dict.get_item("database")?.map_or_else(
            || Err(PyValueError::new_err("Database not set")),
            |s| s.extract(),
        )?;
        let configuration: DataLoadConfiguration = input_dict
            .get_item("configuration")?
            .map_or(Ok(DataLoadConfiguration::default()), |c| c.extract())?;
        let vertex_collections: Vec<CollectionDescription> =
            input_dict.get_item("vertex_collections")?.map_or_else(
                || Err(PyValueError::new_err("vertex_collections not provided")),
                |s| s.extract(),
            )?;
        let edge_collections: Vec<CollectionDescription> =
            input_dict.get_item("edge_collections")?.map_or_else(
                || Err(PyValueError::new_err("edge_collections not provided")),
                |s| s.extract(),
            )?;
        Ok(DataLoadRequest {
            database: database_value.to_string(),
            graph_name: None,
            vertex_collections,
            vertex_attributes: vec![],
            edge_collections,
            configuration,
        })
    }
}

impl FromPyObject<'_> for DataLoadConfiguration {
    fn extract(ob: &PyAny) -> PyResult<Self> {
        let input_dict: &PyDict = ob.downcast()?;
        let db_config: DatabaseConfiguration = input_dict
            .get_item("database_config")?
            .map_or(Ok(DatabaseConfiguration::default()), |c| c.extract())?;
        let parallelism: Option<u32> = input_dict
            .get_item("parallelism")?
            .map_or(Ok(Some(5)), |v| v.extract())?;
        let batch_size: Option<u64> = input_dict
            .get_item("batch_size")?
            .map_or(Ok(Some(400000)), |v| v.extract())?;
        Ok(DataLoadConfiguration {
            database_config: db_config,
            parallelism,
            batch_size,
        })
    }
}

impl FromPyObject<'_> for DatabaseConfiguration {
    fn extract(ob: &'_ PyAny) -> PyResult<Self> {
        let input_dict: &PyDict = ob.downcast()?;
        let endpoints: Vec<String> = input_dict
            .get_item("endpoints")?
            .map_or_else(|| Ok(vec!["http://localhost:8529".into()]), |c| c.extract())?;
        let username: Option<String> = input_dict
            .get_item("username")?
            .map_or_else(|| Ok(Some("root".into())), |c| c.extract())?;
        let password: Option<String> = input_dict
            .get_item("password")?
            .map_or_else(|| Ok(Some("".into())), |c| c.extract())?;
        let jwt_token: Option<String> = input_dict
            .get_item("jwt_token")?
            .map_or_else(|| Ok(None), |c| c.extract())?;
        let tls_cert: Option<String> = input_dict
            .get_item("tls_cert")?
            .map_or_else(|| Ok(None), |c| c.extract())?;
        Ok(DatabaseConfiguration {
            endpoints,
            username,
            password,
            jwt_token,
            tls_cert,
        })
    }
}

impl FromPyObject<'_> for CollectionDescription {
    fn extract(ob: &'_ PyAny) -> PyResult<Self> {
        let input_dict: &PyDict = ob.downcast()?;
        let name: &str = input_dict.get_item("name")?.map_or_else(
            || Err(PyValueError::new_err("Collection name not set")),
            |s| s.extract(),
        )?;
        let fields: Vec<&str> = input_dict
            .get_item("fields")?
            .map_or_else(|| Ok(vec![]), |s| s.extract())?;
        Ok(CollectionDescription {
            name: name.into(),
            fields: fields.iter().map(|s| String::from(*s)).collect(),
        })
    }
}
