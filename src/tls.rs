use rustls::pki_types::{CertificateDer, PrivateKeyDer};
use std::path::Path;

/// TLS configuration.
#[derive(Debug, Clone)]
pub struct TlsConfig {
    pub cert_path: String,
    pub key_path: String,
    pub client_ca_path: Option<String>,
}

fn load_certs(path: &Path) -> anyhow::Result<Vec<CertificateDer<'static>>> {
    let file = std::fs::File::open(path)?;
    let mut reader = std::io::BufReader::new(file);
    let certs = rustls_pemfile::certs(&mut reader).collect::<Result<Vec<_>, _>>()?;
    Ok(certs)
}

fn load_key(path: &Path) -> anyhow::Result<PrivateKeyDer<'static>> {
    let file = std::fs::File::open(path)?;
    let mut reader = std::io::BufReader::new(file);
    let keys = rustls_pemfile::private_key(&mut reader)?;
    keys.ok_or_else(|| anyhow::anyhow!("no private key found"))
}

/// Build a rustls server config. If `client_ca_path` is provided, enables mTLS.
pub fn build_tls_config(config: &TlsConfig) -> anyhow::Result<rustls::ServerConfig> {
    let certs = load_certs(Path::new(&config.cert_path))?;
    let key = load_key(Path::new(&config.key_path))?;

    let mut server_config = if let Some(ref ca_path) = config.client_ca_path {
        // mTLS: require client certificates
        let ca_certs = load_certs(Path::new(ca_path))?;
        let mut root_store = rustls::RootCertStore::empty();
        for cert in ca_certs {
            root_store.add(cert)?;
        }

        let verifier = rustls::server::WebPkiClientVerifier::builder(root_store.into())
            .build()
            .map_err(|e| anyhow::anyhow!("failed to build client verifier: {}", e))?;

        rustls::ServerConfig::builder()
            .with_client_cert_verifier(verifier)
            .with_single_cert(certs, key)?
    } else {
        // TLS only
        rustls::ServerConfig::builder()
            .with_no_client_auth()
            .with_single_cert(certs, key)?
    };

    server_config.alpn_protocols = vec![b"h2".to_vec(), b"http/1.1".to_vec()];
    Ok(server_config)
}

/// Optional TLS config from application Config.
#[derive(Debug, Clone, Default)]
pub struct TlsAppConfig {
    pub enabled: bool,
    pub cert_path: Option<String>,
    pub key_path: Option<String>,
    pub client_ca_path: Option<String>,
}

impl TlsAppConfig {
    pub fn to_tls_config(&self) -> anyhow::Result<Option<TlsConfig>> {
        if !self.enabled {
            return Ok(None);
        }
        let cert_path = self
            .cert_path
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("tls.enabled=true but cert_path is missing"))?
            .clone();
        let key_path = self
            .key_path
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("tls.enabled=true but key_path is missing"))?
            .clone();
        Ok(Some(TlsConfig {
            cert_path,
            key_path,
            client_ca_path: self.client_ca_path.clone(),
        }))
    }
}
