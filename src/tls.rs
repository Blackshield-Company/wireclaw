//! Upstream TLS client configuration shared by the proxy and replay engine.
//!
//! Trusts the operating system's native root store plus, optionally, extra
//! PEM-encoded CA certificates listed in `WIRECLAW_UPSTREAM_CA` (a path, or
//! several separated by the platform path separator). This is useful for
//! internal services signed by a private CA and for local test servers.

use hyper_rustls::{HttpsConnector, HttpsConnectorBuilder};
use hyper_util::client::legacy::connect::HttpConnector;
use rustls::{ClientConfig, RootCertStore};

/// Environment variable naming extra upstream CA bundles to trust.
pub const UPSTREAM_CA_ENV: &str = "WIRECLAW_UPSTREAM_CA";

/// Build the root store: native roots + any extra CAs from the environment.
pub fn root_store() -> RootCertStore {
    let mut roots = RootCertStore::empty();
    let native = rustls_native_certs::load_native_certs();
    for err in &native.errors {
        eprintln!("[wireclaw] warning: could not load some native root certs: {err}");
    }
    let (_added, _ignored) = roots.add_parsable_certificates(native.certs);

    if let Some(paths) = std::env::var_os(UPSTREAM_CA_ENV) {
        for path in std::env::split_paths(&paths) {
            match std::fs::read(&path) {
                Ok(pem) => {
                    let certs = rustls_pemfile::certs(&mut pem.as_slice())
                        .filter_map(|c| c.ok())
                        .collect::<Vec<_>>();
                    let (added, _) = roots.add_parsable_certificates(certs);
                    if added == 0 {
                        eprintln!(
                            "[wireclaw] warning: no certificates found in {}",
                            path.display()
                        );
                    }
                }
                Err(e) => eprintln!(
                    "[wireclaw] warning: cannot read {UPSTREAM_CA_ENV} file {}: {e}",
                    path.display()
                ),
            }
        }
    }

    if roots.is_empty() {
        eprintln!("[wireclaw] warning: no trusted root certificates; upstream HTTPS will fail");
    }
    roots
}

fn client_config() -> ClientConfig {
    ClientConfig::builder()
        .with_root_certificates(root_store())
        .with_no_client_auth()
}

/// Connector accepting both `http://` and `https://` upstreams.
pub fn https_or_http_connector() -> HttpsConnector<HttpConnector> {
    HttpsConnectorBuilder::new()
        .with_tls_config(client_config())
        .https_or_http()
        .enable_http1()
        .enable_http2()
        .build()
}

/// Connector that only allows `https://` upstreams.
pub fn https_only_connector() -> HttpsConnector<HttpConnector> {
    HttpsConnectorBuilder::new()
        .with_tls_config(client_config())
        .https_only()
        .enable_http1()
        .enable_http2()
        .build()
}
