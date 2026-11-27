use crate::DatabaseConfiguration;
use reqwest_middleware::RequestBuilder;

pub fn handle_auth(
    request_builder: RequestBuilder,
    db_config: &DatabaseConfiguration,
) -> RequestBuilder {
    if db_config.jwt_token.is_empty() {
        handle_basic_auth(request_builder, &db_config)
    } else {
        request_builder.bearer_auth(db_config.jwt_token.clone())
    }
}

/// Authentication for the deployment detection calls (`/_api/version`,
/// `/_admin/support-info`, `/_admin/cluster/shardDistribution`): the
/// metadata token if one is configured, otherwise the main credentials.
/// Never use this for requests that read collection data.
pub fn handle_metadata_auth(
    request_builder: RequestBuilder,
    db_config: &DatabaseConfiguration,
) -> RequestBuilder {
    match &db_config.metadata_jwt_token {
        Some(token) if !token.is_empty() => request_builder.bearer_auth(token.clone()),
        _ => handle_auth(request_builder, db_config),
    }
}

fn handle_basic_auth(
    request_builder: RequestBuilder,
    db_config: &&DatabaseConfiguration,
) -> RequestBuilder {
    if db_config.username.is_empty() {
        request_builder
    } else {
        request_builder.basic_auth(db_config.username.clone(), Some(db_config.password.clone()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::DatabaseConfigurationBuilder;
    use reqwest::header::AUTHORIZATION;

    fn authorization(
        auth: fn(RequestBuilder, &DatabaseConfiguration) -> RequestBuilder,
        db_config: &DatabaseConfiguration,
    ) -> Option<String> {
        let client = reqwest_middleware::ClientBuilder::new(reqwest::Client::new()).build();
        let request = auth(client.get("http://localhost:8529/_api/version"), db_config)
            .build()
            .expect("request builds");
        request
            .headers()
            .get(AUTHORIZATION)
            .map(|v| v.to_str().unwrap().to_string())
    }

    #[test]
    fn metadata_calls_use_the_metadata_token_when_set() {
        let db_config = DatabaseConfigurationBuilder::new()
            .jwt_token("user".to_string())
            .metadata_jwt_token("service".to_string())
            .build();
        assert_eq!(
            authorization(handle_metadata_auth, &db_config).as_deref(),
            Some("Bearer service")
        );
        // Data requests keep the main credentials.
        assert_eq!(
            authorization(handle_auth, &db_config).as_deref(),
            Some("Bearer user")
        );
    }

    #[test]
    fn metadata_calls_fall_back_to_the_main_credentials() {
        let db_config = DatabaseConfigurationBuilder::new()
            .jwt_token("user".to_string())
            .build();
        assert_eq!(
            authorization(handle_metadata_auth, &db_config).as_deref(),
            Some("Bearer user")
        );
        let empty = DatabaseConfigurationBuilder::new()
            .jwt_token("user".to_string())
            .metadata_jwt_token(String::new())
            .build();
        assert_eq!(
            authorization(handle_metadata_auth, &empty).as_deref(),
            Some("Bearer user")
        );
        let basic = DatabaseConfigurationBuilder::new()
            .username("alice".to_string())
            .password("secret".to_string())
            .build();
        assert_eq!(
            authorization(handle_metadata_auth, &basic),
            authorization(handle_auth, &basic)
        );
        assert!(
            authorization(handle_metadata_auth, &basic)
                .unwrap()
                .starts_with("Basic ")
        );
    }
}
