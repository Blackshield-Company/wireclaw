//! HTTPS interception tests: CA generation, per-host certificate signing and
//! full MITM capture of TLS traffic to a local HTTPS server.

mod support;
use support::*;

#[test]
fn ca_generate_creates_persistent_ca() {
    let env = Env::new();
    let out = env.run(&["ca", "generate"]);
    let printed = String::from_utf8_lossy(&out.stdout);
    let ca_path = env.ca_cert_path();
    let on_disk = std::fs::read_to_string(&ca_path).expect("ca.crt written");
    assert!(on_disk.starts_with("-----BEGIN CERTIFICATE-----"));
    assert!(
        printed.contains(on_disk.trim()),
        "generate prints the CA PEM"
    );
    assert!(env.data_dir().join("certs").join("ca.key").exists());

    // Parse and check it really is a CA.
    let der = rustls_pemfile::certs(&mut on_disk.as_bytes())
        .next()
        .unwrap()
        .unwrap();
    let (_, cert) = x509_parser::parse_x509_certificate(&der).unwrap();
    assert!(cert.is_ca(), "CA must have basicConstraints CA:TRUE");
    assert!(cert.subject().to_string().contains("Wireclaw Proxy CA"));

    // Second run reuses the same CA instead of regenerating.
    let again = env.run(&["ca", "show"]);
    assert_eq!(
        String::from_utf8_lossy(&again.stdout).trim(),
        on_disk.trim()
    );

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(env.data_dir().join("certs").join("ca.key"))
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o600, "CA private key must be owner-only");
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn mitm_captures_decrypted_https_traffic() {
    let env_dir = tempfile::tempdir().unwrap();
    let (upstream, seen, upstream_ca) = start_https_upstream(env_dir.path()).await;
    let mut env = Env::new();
    env.extra_env.push((
        "WIRECLAW_UPSTREAM_CA".into(),
        upstream_ca.display().to_string(),
    ));
    let proxy = env.capture("tls");

    let body = r#"{"secret":"inside-tls"}"#;
    let r = https_via_proxy(
        proxy.addr,
        "localhost",
        upstream.port(),
        &env.ca_cert_path(),
        "POST",
        "/login?x=1",
        Some(body),
    )
    .await;
    assert_eq!(r.status, 200, "{r:?}");
    assert_eq!(r.json()["body"], body);
    assert_eq!(seen.lock().unwrap()[0].path, "/login?x=1");

    let list = env.wait_list("tls", |t| t.contains("POST /login"));
    assert!(
        list.contains(r#"body: {"secret":"inside-tls"}"#),
        "plaintext captured:\n{list}"
    );

    // The stored URL is the real https:// URL.
    drop(proxy);
    let har_path = env.dir.path().join("tls.har");
    env.run(&[
        "export",
        "-f",
        "har",
        "-s",
        "tls",
        "-o",
        har_path.to_str().unwrap(),
    ]);
    let har: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&har_path).unwrap()).unwrap();
    assert_eq!(
        har["log"]["entries"][0]["request"]["url"],
        format!("https://localhost:{}/login?x=1", upstream.port())
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn leaf_cert_is_signed_by_ca_for_requested_host() {
    let env_dir = tempfile::tempdir().unwrap();
    let (upstream, _, upstream_ca) = start_https_upstream(env_dir.path()).await;
    let mut env = Env::new();
    env.extra_env.push((
        "WIRECLAW_UPSTREAM_CA".into(),
        upstream_ca.display().to_string(),
    ));
    let proxy = env.capture("leaf");

    let tls = connect_tls(
        proxy.addr,
        "localhost",
        upstream.port(),
        &env.ca_cert_path(),
    )
    .await
    .expect("client trusting only the wireclaw CA must accept the leaf");
    let (_, conn) = tls.get_ref();
    let chain = conn.peer_certificates().unwrap();
    let (_, leaf) = x509_parser::parse_x509_certificate(&chain[0]).unwrap();
    assert!(!leaf.is_ca());
    assert!(leaf.issuer().to_string().contains("Wireclaw Proxy CA"));
    let san = leaf.subject_alternative_name().unwrap().unwrap();
    assert!(
        format!("{:?}", san.value.general_names).contains("localhost"),
        "leaf SAN must cover the CONNECT host"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn mitm_works_for_ip_literal_hosts() {
    // Upstream cert is for "localhost", so trust the TLS session to 127.0.0.1
    // only on the client<->proxy leg; the upstream leg is expected to fail
    // verification and surface as 502, but the client handshake must succeed
    // with an IP-address SAN.
    let env_dir = tempfile::tempdir().unwrap();
    let (upstream, _, upstream_ca) = start_https_upstream(env_dir.path()).await;
    let mut env = Env::new();
    env.extra_env.push((
        "WIRECLAW_UPSTREAM_CA".into(),
        upstream_ca.display().to_string(),
    ));
    let proxy = env.capture("ip");
    let r = https_via_proxy(
        proxy.addr,
        "127.0.0.1",
        upstream.port(),
        &env.ca_cert_path(),
        "GET",
        "/",
        None,
    )
    .await;
    assert_eq!(r.status, 502, "upstream cert has no IP SAN: {r:?}");
}

#[tokio::test(flavor = "multi_thread")]
async fn client_not_trusting_ca_rejects_mitm() {
    let env_dir = tempfile::tempdir().unwrap();
    let (upstream, seen, upstream_ca) = start_https_upstream(env_dir.path()).await;
    let env = Env::new();
    let proxy = env.capture("untrusted");
    // Trust the *upstream's* cert instead of the wireclaw CA: must fail.
    let res = connect_tls(proxy.addr, "localhost", upstream.port(), &upstream_ca).await;
    assert!(
        res.is_err(),
        "handshake must fail without trusting the wireclaw CA"
    );
    assert!(seen.lock().unwrap().is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn ca_survives_restart_and_reloaded_ca_still_signs_valid_leaves() {
    let env_dir = tempfile::tempdir().unwrap();
    let (upstream, _, upstream_ca) = start_https_upstream(env_dir.path()).await;
    let mut env = Env::new();
    env.extra_env.push((
        "WIRECLAW_UPSTREAM_CA".into(),
        upstream_ca.display().to_string(),
    ));
    env.run(&["ca", "generate"]);
    let before = std::fs::read_to_string(env.ca_cert_path()).unwrap();

    // A fresh process loads the CA from disk; leaves it signs must verify
    // against the ca.crt users installed earlier.
    let proxy = env.capture("restart");
    assert_eq!(std::fs::read_to_string(env.ca_cert_path()).unwrap(), before);
    let r = https_via_proxy(
        proxy.addr,
        "localhost",
        upstream.port(),
        &env.ca_cert_path(),
        "GET",
        "/after-restart",
        None,
    )
    .await;
    assert_eq!(r.status, 200, "{r:?}");
}

#[tokio::test(flavor = "multi_thread")]
async fn upstream_with_untrusted_cert_is_rejected() {
    // Without WIRECLAW_UPSTREAM_CA the proxy must NOT silently accept a
    // self-signed upstream: it reports 502 to the client.
    let env_dir = tempfile::tempdir().unwrap();
    let (upstream, seen, _) = start_https_upstream(env_dir.path()).await;
    let env = Env::new();
    let proxy = env.capture("strict");
    let r = https_via_proxy(
        proxy.addr,
        "localhost",
        upstream.port(),
        &env.ca_cert_path(),
        "GET",
        "/",
        None,
    )
    .await;
    assert_eq!(r.status, 502, "{r:?}");
    assert!(seen.lock().unwrap().is_empty());
}
