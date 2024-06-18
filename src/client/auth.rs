use crate::input::load_request::DatabaseConfiguration;
use reqwest_middleware::RequestBuilder;

pub fn handle_auth(
    request_builder: RequestBuilder,
    db_config: &DatabaseConfiguration,
) -> RequestBuilder {
    match &db_config.jwt_token {
        Some(token) => request_builder.bearer_auth(token),
        None => handle_basic_auth(request_builder, &db_config),
    }
}

fn handle_basic_auth(
    request_builder: RequestBuilder,
    db_config: &&DatabaseConfiguration,
) -> RequestBuilder {
    match &db_config.username {
        Some(username) => request_builder.basic_auth(username, db_config.password.as_ref()),
        None => {
            // proceed without authentication
            request_builder
        }
    }
}
