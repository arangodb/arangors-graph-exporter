use reqwest_middleware::RequestBuilder;
use crate::DatabaseConfiguration;

pub fn handle_auth(
    request_builder: RequestBuilder,
    db_config: &DatabaseConfiguration,
) -> RequestBuilder {
    if db_config.jwt_token.is_empty() {
        return handle_basic_auth(request_builder, &db_config);
    } else {
        request_builder.bearer_auth(db_config.jwt_token.clone())
    }
}

fn handle_basic_auth(
    request_builder: RequestBuilder,
    db_config: &&DatabaseConfiguration,
) -> RequestBuilder {
    if db_config.username.is_empty() {
        return request_builder;
    } else {
        request_builder.basic_auth(db_config.username.clone(), Some(db_config.password.clone()))
    }
}
