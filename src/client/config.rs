#[derive(Debug)]
pub struct ClientConfig {
    pub n_retries: u32,
    pub tls_cert: Option<String>,
    pub use_tls: bool,
}

impl ClientConfig {
    pub fn builder() -> ClientConfigBuilder {
        ClientConfigBuilder::new()
    }
}

pub struct ClientConfigBuilder {
    use_tls: bool,
    tls_cert: Option<String>,
    n_retries: Option<u32>,
}

impl Default for ClientConfigBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl ClientConfigBuilder {
    pub fn new() -> ClientConfigBuilder {
        ClientConfigBuilder {
            n_retries: None,
            tls_cert: None,
            use_tls: false,
        }
    }

    pub fn n_retries(mut self, n: u32) -> ClientConfigBuilder {
        self.n_retries = Some(n);
        self
    }

    pub fn tls_cert_opt(mut self, cert: Option<String>) -> ClientConfigBuilder {
        self.tls_cert = cert;
        self
    }

    pub fn use_tls(mut self, use_tls: bool) -> ClientConfigBuilder {
        self.use_tls = use_tls;
        self
    }

    pub fn build(self) -> ClientConfig {
        ClientConfig {
            n_retries: self.n_retries.unwrap_or(5), // 5 retries by default
            tls_cert: self.tls_cert,
            use_tls: self.use_tls,
        }
    }
}
