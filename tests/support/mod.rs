//! Shared helpers for end-to-end tests: local upstream servers (HTTP and
//! HTTPS), a managed `wireclaw capture` child process, and minimal raw
//! HTTP/1.1 clients so tests exercise the proxy exactly like a real client.

#![allow(dead_code)]

use std::convert::Infallible;
use std::net::{SocketAddr, TcpListener as StdListener};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Output, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use http_body_util::{BodyExt, Full};
use hyper::body::{Bytes, Incoming};
use hyper::service::service_fn;
use hyper::{Request, Response};
use hyper_util::rt::TokioIo;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

pub const BIN: &str = env!("CARGO_BIN_EXE_wireclaw");

/// A request observed by an upstream test server.
#[derive(Debug, Clone)]
pub struct Seen {
    pub method: String,
    pub path: String,
    pub body: String,
}

pub type SeenLog = Arc<Mutex<Vec<Seen>>>;

pub fn free_port() -> u16 {
    StdListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}

async fn echo(req: Request<Incoming>, log: SeenLog) -> Result<Response<Full<Bytes>>, Infallible> {
    let method = req.method().to_string();
    let path = req
        .uri()
        .path_and_query()
        .map(|p| p.as_str().to_string())
        .unwrap_or_default();
    let body = req.into_body().collect().await.unwrap().to_bytes();
    let body = String::from_utf8_lossy(&body).to_string();
    let n = {
        let mut l = log.lock().unwrap();
        l.push(Seen {
            method: method.clone(),
            path: path.clone(),
            body: body.clone(),
        });
        l.len()
    };
    let json = serde_json::json!({ "method": method, "path": path, "body": body, "n": n });
    Ok(Response::builder()
        .status(200)
        .header("content-type", "application/json")
        .header("x-upstream", "wireclaw-test")
        .body(Full::new(Bytes::from(json.to_string())))
        .unwrap())
}

/// A TCP server that accepts and immediately closes every connection.
pub async fn start_dead_upstream() -> SocketAddr {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        while let Ok((s, _)) = listener.accept().await {
            drop(s);
        }
    });
    addr
}

/// Start a plain HTTP echo server on 127.0.0.1. Returns its address and log.
pub async fn start_http_upstream() -> (SocketAddr, SeenLog) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let log: SeenLog = Arc::default();
    let l2 = log.clone();
    tokio::spawn(async move {
        loop {
            let Ok((stream, _)) = listener.accept().await else {
                return;
            };
            let log = l2.clone();
            tokio::spawn(async move {
                let svc = service_fn(move |r| echo(r, log.clone()));
                let _ = hyper::server::conn::http1::Builder::new()
                    .serve_connection(TokioIo::new(stream), svc)
                    .await;
            });
        }
    });
    (addr, log)
}

/// Start an HTTPS echo server for `localhost` with a fresh self-signed cert.
/// Returns its address, log, and the path to the PEM cert to trust.
pub async fn start_https_upstream(dir: &Path) -> (SocketAddr, SeenLog, PathBuf) {
    let ck = rcgen::generate_simple_self_signed(vec!["localhost".to_string()]).unwrap();
    let cert_path = dir.join("upstream.crt");
    std::fs::write(&cert_path, ck.cert.pem()).unwrap();
    let cert_der = ck.cert.der().clone();
    let key_der =
        rustls::pki_types::PrivateKeyDer::try_from(ck.signing_key.serialize_der()).unwrap();
    let mut cfg = rustls::ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(vec![cert_der], key_der)
        .unwrap();
    cfg.alpn_protocols = vec![b"http/1.1".to_vec()];
    let acceptor = tokio_rustls::TlsAcceptor::from(Arc::new(cfg));

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let log: SeenLog = Arc::default();
    let l2 = log.clone();
    tokio::spawn(async move {
        loop {
            let Ok((stream, _)) = listener.accept().await else {
                return;
            };
            let acceptor = acceptor.clone();
            let log = l2.clone();
            tokio::spawn(async move {
                let Ok(tls) = acceptor.accept(stream).await else {
                    return;
                };
                let svc = service_fn(move |r| echo(r, log.clone()));
                let _ = hyper::server::conn::http1::Builder::new()
                    .serve_connection(TokioIo::new(tls), svc)
                    .await;
            });
        }
    });
    (addr, log, cert_path)
}

/// An isolated wireclaw environment (data dir + config path) for one test.
pub struct Env {
    pub dir: tempfile::TempDir,
    pub extra_env: Vec<(String, String)>,
}

impl Env {
    pub fn new() -> Self {
        Self {
            dir: tempfile::tempdir().unwrap(),
            extra_env: Vec::new(),
        }
    }

    pub fn data_dir(&self) -> PathBuf {
        self.dir.path().join("data")
    }

    pub fn ca_cert_path(&self) -> PathBuf {
        self.data_dir().join("certs").join("ca.crt")
    }

    pub fn command(&self) -> Command {
        let mut cmd = Command::new(BIN);
        cmd.env("WIRECLAW_DATA_DIR", self.data_dir())
            .env("WIRECLAW_CONFIG", self.dir.path().join("no-config.toml"))
            .env_remove("WIRECLAW_ADDR")
            .env_remove("WIRECLAW_UPSTREAM_CA")
            // Keep any system proxy settings away from the replay client.
            .env_remove("HTTP_PROXY")
            .env_remove("HTTPS_PROXY")
            .env_remove("http_proxy")
            .env_remove("https_proxy");
        for (k, v) in &self.extra_env {
            cmd.env(k, v);
        }
        cmd
    }

    /// Run a wireclaw subcommand to completion.
    pub fn run(&self, args: &[&str]) -> Output {
        let out = self.command().args(args).output().unwrap();
        assert!(
            out.status.success(),
            "wireclaw {args:?} failed: {}\n{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
        out
    }

    /// Start `wireclaw capture` on an OS-assigned port and wait until the
    /// proxy reports the address it bound.
    pub fn capture(&self, session: &str) -> Proxy {
        use std::io::BufRead;
        let mut child = self
            .command()
            .args(["--addr", "127.0.0.1:0", "capture", "--session", session])
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let stderr = child.stderr.take().unwrap();
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let mut tx = Some(tx);
            for line in std::io::BufReader::new(stderr).lines() {
                let Ok(line) = line else { break };
                if let Some(addr) = line.strip_prefix("[wireclaw] proxy listening on ")
                    && let Some(tx) = tx.take()
                {
                    let _ = tx.send(addr.trim().to_string());
                }
            }
        });
        let addr = rx
            .recv_timeout(Duration::from_secs(30))
            .expect("proxy did not report its listening address");
        Proxy {
            child,
            addr: addr.parse().unwrap(),
        }
    }

    /// `wireclaw list --headers` output (stderr), polled until `pred` holds.
    pub fn wait_list(&self, session: &str, pred: impl Fn(&str) -> bool) -> String {
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            let out = self.run(&["list", "--session", session, "--headers", "--bodies"]);
            let text = String::from_utf8_lossy(&out.stderr).to_string();
            if pred(&text) {
                return text;
            }
            assert!(
                Instant::now() < deadline,
                "condition not met; list:\n{text}"
            );
            std::thread::sleep(Duration::from_millis(100));
        }
    }
}

pub struct Proxy {
    pub child: Child,
    pub addr: SocketAddr,
}

impl Drop for Proxy {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// Minimal parsed HTTP/1.1 response.
#[derive(Debug)]
pub struct RawResponse {
    pub status: u16,
    pub head: String,
    pub body: String,
}

impl RawResponse {
    pub fn json(&self) -> serde_json::Value {
        serde_json::from_str(&self.body)
            .unwrap_or_else(|e| panic!("body is not JSON ({e}): {}", self.body))
    }
}

async fn read_head<S: AsyncRead + Unpin>(s: &mut S) -> (String, Vec<u8>) {
    let mut buf = Vec::new();
    let mut chunk = [0u8; 4096];
    loop {
        if let Some(pos) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
            let head = String::from_utf8_lossy(&buf[..pos]).to_string();
            return (head, buf[pos + 4..].to_vec());
        }
        let n = s.read(&mut chunk).await.unwrap();
        assert!(n > 0, "connection closed before headers");
        buf.extend_from_slice(&chunk[..n]);
    }
}

/// Read a response that carries `Content-Length` (or ends at EOF).
pub async fn read_response<S: AsyncRead + Unpin>(s: &mut S) -> RawResponse {
    let (head, mut rest) = read_head(s).await;
    let status: u16 = head
        .split_whitespace()
        .nth(1)
        .and_then(|c| c.parse().ok())
        .unwrap_or_else(|| panic!("bad status line: {head}"));
    let len = head.lines().find_map(|l| {
        let (k, v) = l.split_once(':')?;
        k.trim()
            .eq_ignore_ascii_case("content-length")
            .then(|| v.trim().parse::<usize>().ok())?
    });
    let mut chunk = [0u8; 4096];
    match len {
        Some(len) => {
            while rest.len() < len {
                let n = s.read(&mut chunk).await.unwrap();
                assert!(n > 0, "connection closed mid-body");
                rest.extend_from_slice(&chunk[..n]);
            }
            rest.truncate(len);
        }
        None => {
            while let Ok(n) = s.read(&mut chunk).await {
                if n == 0 {
                    break;
                }
                rest.extend_from_slice(&chunk[..n]);
            }
        }
    }
    RawResponse {
        status,
        head,
        body: String::from_utf8_lossy(&rest).to_string(),
    }
}

fn request_bytes(method: &str, target: &str, host: &str, body: Option<&str>) -> Vec<u8> {
    let mut req = format!("{method} {target} HTTP/1.1\r\nHost: {host}\r\nConnection: close\r\n");
    if let Some(b) = body {
        req.push_str(&format!(
            "Content-Type: application/json\r\nContent-Length: {}\r\n",
            b.len()
        ));
    }
    req.push_str("\r\n");
    if let Some(b) = body {
        req.push_str(b);
    }
    req.into_bytes()
}

async fn send<S: AsyncRead + AsyncWrite + Unpin>(s: &mut S, req: &[u8]) -> RawResponse {
    s.write_all(req).await.unwrap();
    s.flush().await.unwrap();
    read_response(s).await
}

/// Send a plain-HTTP request through the proxy (absolute-form target).
pub async fn http_via_proxy(
    proxy: SocketAddr,
    method: &str,
    upstream: SocketAddr,
    path: &str,
    body: Option<&str>,
) -> RawResponse {
    let mut s = TcpStream::connect(proxy).await.unwrap();
    let target = format!("http://{upstream}{path}");
    send(
        &mut s,
        &request_bytes(method, &target, &upstream.to_string(), body),
    )
    .await
}

/// Open a CONNECT tunnel through the proxy and complete a TLS handshake that
/// trusts only `ca_pem_path`. Returns the TLS stream, or the handshake error.
pub async fn connect_tls(
    proxy: SocketAddr,
    host: &str,
    port: u16,
    ca_pem_path: &Path,
) -> std::io::Result<tokio_rustls::client::TlsStream<TcpStream>> {
    let mut s = TcpStream::connect(proxy).await?;
    let authority = format!("{host}:{port}");
    s.write_all(format!("CONNECT {authority} HTTP/1.1\r\nHost: {authority}\r\n\r\n").as_bytes())
        .await?;
    let (head, rest) = read_head(&mut s).await;
    assert!(head.contains(" 200"), "CONNECT failed: {head}");
    assert!(rest.is_empty());

    let mut roots = rustls::RootCertStore::empty();
    let pem = std::fs::read(ca_pem_path)?;
    for c in rustls_pemfile::certs(&mut pem.as_slice()) {
        roots.add(c?).unwrap();
    }
    let cfg = rustls::ClientConfig::builder()
        .with_root_certificates(roots)
        .with_no_client_auth();
    let connector = tokio_rustls::TlsConnector::from(Arc::new(cfg));
    let name = rustls::pki_types::ServerName::try_from(host.to_string()).unwrap();
    connector.connect(name, s).await
}

/// Send one HTTPS request through the proxy's MITM tunnel.
pub async fn https_via_proxy(
    proxy: SocketAddr,
    host: &str,
    port: u16,
    ca_pem_path: &Path,
    method: &str,
    path: &str,
    body: Option<&str>,
) -> RawResponse {
    let mut tls = connect_tls(proxy, host, port, ca_pem_path)
        .await
        .expect("TLS handshake through proxy failed");
    send(
        &mut tls,
        &request_bytes(method, path, &format!("{host}:{port}"), body),
    )
    .await
}
