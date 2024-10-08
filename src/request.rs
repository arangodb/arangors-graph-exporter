use crate::errors::GraphLoaderError;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ArangoDBError {
    error: bool,
    error_num: i32,
    error_message: String,
    code: i32,
}

// This function handles an HTTP response from ArangoDB, including
// connection errors, bad status codes and body parsing. The template
// type is the type of the expected body in the good case.
pub async fn handle_arangodb_response_with_parsed_body<T>(
    resp: reqwest_middleware::Result<reqwest::Response>,
    expected_code: reqwest::StatusCode,
) -> Result<T, GraphLoaderError>
where
    T: serde::de::DeserializeOwned,
{
    let resp = resp.map_err(GraphLoaderError::RequestError)?; // Convert reqwest::Error to GraphLoaderError::RequestError

    let status = resp.status();
    if status != expected_code {
        let arango_error = resp.json::<ArangoDBError>().await.map_err(|err| {
            GraphLoaderError::ParseError(format!("Error parsing response body: {}", err))
        })?;

        return Err(GraphLoaderError::ArangoDBError(
            arango_error.error_num,
            arango_error.error_message,
            status,
        ));
    }

    resp.json::<T>().await.map_err(|err| {
        GraphLoaderError::ParseError(format!("Error parsing response body: {}", err))
    })
}

// This function handles an empty HTTP response from ArangoDB, including
// connection errors and bad status codes.
pub async fn handle_arangodb_response(
    resp: reqwest_middleware::Result<reqwest::Response>,
    code_test: fn(code: reqwest::StatusCode) -> bool,
) -> Result<reqwest::Response, String> {
    if let Err(err) = resp {
        return Err(err.to_string());
    }
    let resp = resp.unwrap();
    handle_arangodb_req_response(resp, code_test).await
}

async fn handle_arangodb_req_response(
    resp: reqwest::Response,
    code_test: fn(code: reqwest::StatusCode) -> bool,
) -> Result<reqwest::Response, String> {
    let status = resp.status();
    if !code_test(status) {
        let err = resp.json::<ArangoDBError>().await;
        match err {
            Err(e) => {
                return Err(format!(
                    "Could not parse error body, error: {}, status code: {:?}",
                    e, status,
                ));
            }
            Ok(e) => {
                return Err(format!(
                    "Error code: {}, message: {}, HTTP code: {}",
                    e.error_num, e.error_message, e.code
                ));
            }
        }
    }
    Ok(resp)
}
