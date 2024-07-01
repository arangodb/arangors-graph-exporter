use crate::{DatabaseConfiguration};
use crate::errors::GraphLoaderError;

pub(crate) fn build_client(db_config: &DatabaseConfiguration) -> Result<reqwest::Client, GraphLoaderError> {
    let endpoints = &db_config.endpoints;
    let tls_cert = &db_config.tls_cert;
    let use_tls = endpoints[0].starts_with("https://");

    let mut builder = reqwest::Client::builder();
    if use_tls {
        builder = builder
            .use_rustls_tls()
            .min_tls_version(reqwest::tls::Version::TLS_1_2)
            .danger_accept_invalid_certs(true)
            .https_only(true);

        if !tls_cert.is_empty() {
            let cert = reqwest::Certificate::from_pem(&tls_cert);
            if let Err(err) = cert {
                return Err(GraphLoaderError::TlsCertError(format!(
                    "Could not parse CA certificate for ArangoDB: {:?}",
                    err
                )));
            }
            builder = builder.add_root_certificate(cert.unwrap());
        }
    }

    let client = builder.build().map_err(|err| {
        GraphLoaderError::RequestBuilderError(format!(
            "Error message from request builder: {:?}",
            err
        ))
    })?;

    Ok(client)
}

pub(crate) fn make_url(db_config: &DatabaseConfiguration, path: &str) -> String {
    db_config.endpoints[0].clone() + "/_db/" + &db_config.database + path
}