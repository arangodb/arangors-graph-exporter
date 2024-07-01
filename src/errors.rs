use thiserror::Error;
use reqwest::Error as ReqwestError; // Alias to disambiguate from other error types

#[derive(Error, Debug)]
pub enum GraphLoaderError {
    #[error("Both vertex and edge collections are empty.")]
    EmptyCollections,

    #[error("Error parsing TLS certificate: {0}")]
    TlsCertError(String),

    #[error("Error building request client: {0}")]
    RequestBuilderError(String),

    #[error("No database servers found")]
    NoDatabaseServers,

    #[error("UTF-8 error: {0}")]
    Utf8Error(String),

    #[error("Request error: {0}")]
    RequestError(#[from] ReqwestError),

    #[error("ArangoDB error: code {0}, message: {1}, HTTP status code: {2}")]
    ArangoDBError(i32, String, reqwest::StatusCode),

    #[error("Error parsing response body: {0}")]
    ParseError(String),

    #[error("Invalid HTTP status code: {0}")]
    InvalidStatusCode(reqwest::StatusCode),

    #[error("Graph is not an object")]
    GraphNotObject,

    #[error("No edge definitions found")]
    NoEdgeDefinitions,

    #[error("Edge definitions found, but an array")]
    EdgeDefinitionsNotArray,

    #[error("Collection is not a string")]
    CollectionNotString,

    #[error("From is not an array")]
    FromNotArray,

    #[error("From collection is not a string")]
    FromCollectionNotString,

    #[error("To is not an array")]
    ToNotArray,

    #[error("To collection is not a string")]
    ToCollectionNotString,

    #[error("JSON parse error:\n{0}")]
    JsonParseError(String),

    #[error("{0}")]
    Other(String), // Variant to handle generic String errors
}

impl From<String> for GraphLoaderError {
    fn from(err: String) -> Self {
        GraphLoaderError::Other(err)
    }
}