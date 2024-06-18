use crate::client::config::ClientConfig;
use reqwest::{Certificate, Client};
use reqwest_middleware::{ClientBuilder, ClientWithMiddleware};
use reqwest_retry::policies::ExponentialBackoff;
use reqwest_retry::RetryTransientMiddleware;
use std::fs::File;
use std::io::Read;

pub fn build_client(
    config: &ClientConfig,
) -> Result<reqwest_middleware::ClientWithMiddleware, String> {
    let mut client_builder = reqwest::Client::builder();
    client_builder = if config.use_tls {
        client_builder = client_builder
            .min_tls_version(reqwest::tls::Version::TLS_1_2)
            .https_only(true);

        if let Some(cert_path) = &config.tls_cert {
            let cert = get_cert(cert_path)?;
            client_builder.add_root_certificate(cert).use_rustls_tls()
        } else {
            client_builder
        }
    } else {
        client_builder
    };
    let client = client_builder
        .build()
        .map_err(|err| format!("Error message from request builder: {:?}", err))?;
    let client = client_with_retries(&config.n_retries, client);
    Ok(client)
}

fn get_cert(cert_path: &String) -> Result<Certificate, String> {
    let mut cert_buf = vec![];
    let mut file = File::open(cert_path)
        .map_err(|err| format!("Error message from reading TLS certificate: {:?}", err))?;
    file.read_to_end(&mut cert_buf)
        .map_err(|err| format!("Error message from reading TLS certificate: {:?}", err))?;
    let cert = reqwest::Certificate::from_pem(&cert_buf)
        .map_err(|err| format!("Error message from request builder: {:?}", err))?;
    Ok(cert)
}

fn client_with_retries(n_retries: &u32, client: Client) -> ClientWithMiddleware {
    let retry_policy = ExponentialBackoff::builder()
        .retry_bounds(
            std::time::Duration::from_millis(30),
            std::time::Duration::from_millis(3000),
        )
        .build_with_max_retries(*n_retries);
    let retry_middleware = RetryTransientMiddleware::new_with_policy(retry_policy);
    ClientBuilder::new(client).with(retry_middleware).build()
}
