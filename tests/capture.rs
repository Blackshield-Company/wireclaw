//! End-to-end capture tests: plain HTTP through `wireclaw capture` into SQLite,
//! verified via `wireclaw list` and the HAR export.

mod support;
use support::*;

#[tokio::test(flavor = "multi_thread")]
async fn captures_http_get_and_post_through_proxy() {
    let (upstream, seen) = start_http_upstream().await;
    let env = Env::new();
    let proxy = env.capture("cap");

    let r = http_via_proxy(proxy.addr, "GET", upstream, "/users?page=2", None).await;
    assert_eq!(r.status, 200, "{r:?}");
    assert_eq!(r.json()["path"], "/users?page=2");
    assert!(r.head.to_lowercase().contains("x-upstream: wireclaw-test"));

    let body = r#"{"name":"claw"}"#;
    let r = http_via_proxy(proxy.addr, "POST", upstream, "/users", Some(body)).await;
    assert_eq!(r.status, 200);
    assert_eq!(
        r.json()["body"],
        body,
        "proxy must forward the request body"
    );

    {
        let seen = seen.lock().unwrap();
        assert_eq!(seen.len(), 2);
        assert_eq!(seen[1].method, "POST");
        assert_eq!(seen[1].body, body);
    }

    let list = env.wait_list("cap", |t| {
        t.contains("GET /users") && t.contains("POST /users")
    });
    assert!(
        list.contains("POST /users 2xx"),
        "status should be recorded:\n{list}"
    );
    assert!(
        list.contains(r#"body: {"name":"claw"}"#),
        "request body stored:\n{list}"
    );
    assert!(list.contains("resp body:"), "response body stored:\n{list}");
}

#[tokio::test(flavor = "multi_thread")]
async fn sessions_are_isolated() {
    let (upstream, _) = start_http_upstream().await;
    let env = Env::new();
    {
        let proxy = env.capture("alpha");
        http_via_proxy(proxy.addr, "GET", upstream, "/only-alpha", None).await;
        env.wait_list("alpha", |t| t.contains("/only-alpha"));
    }
    let proxy = env.capture("beta");
    http_via_proxy(proxy.addr, "GET", upstream, "/only-beta", None).await;
    let beta = env.wait_list("beta", |t| t.contains("/only-beta"));
    assert!(
        !beta.contains("/only-alpha"),
        "beta leaked alpha traffic:\n{beta}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn unreachable_upstream_yields_502_not_a_crash() {
    let env = Env::new();
    let proxy = env.capture("down");
    let dead = start_dead_upstream().await;
    let r = http_via_proxy(proxy.addr, "GET", dead, "/nowhere", None).await;
    assert_eq!(r.status, 502, "{r:?}");

    // Proxy keeps serving afterwards.
    let (upstream, _) = start_http_upstream().await;
    let r = http_via_proxy(proxy.addr, "GET", upstream, "/alive", None).await;
    assert_eq!(r.status, 200);
}

#[tokio::test(flavor = "multi_thread")]
async fn har_export_is_valid_har_1_2() {
    let (upstream, _) = start_http_upstream().await;
    let env = Env::new();
    let proxy = env.capture("har");
    http_via_proxy(proxy.addr, "POST", upstream, "/items", Some(r#"{"x":1}"#)).await;
    http_via_proxy(proxy.addr, "GET", upstream, "/items/1", None).await;
    env.wait_list("har", |t| {
        t.contains("/items/1") && t.contains("POST /items")
    });
    drop(proxy);

    let out = env.dir.path().join("out.har");
    env.run(&[
        "export",
        "--format",
        "har",
        "--session",
        "har",
        "--output",
        out.to_str().unwrap(),
    ]);
    let har: serde_json::Value = serde_json::from_slice(&std::fs::read(&out).unwrap()).unwrap();
    assert_eq!(har["log"]["version"], "1.2");
    assert!(har["log"]["creator"]["name"].is_string());
    let entries = har["log"]["entries"].as_array().unwrap();
    assert_eq!(entries.len(), 2);
    let post = entries
        .iter()
        .find(|e| e["request"]["method"] == "POST")
        .expect("POST entry");
    assert_eq!(post["request"]["url"], format!("http://{upstream}/items"));
    assert_eq!(post["request"]["postData"]["text"], r#"{"x":1}"#);
    assert_eq!(post["response"]["status"], 200);
    assert!(
        post["response"]["content"]["text"]
            .as_str()
            .unwrap()
            .contains("\"method\":\"POST\"")
    );
    for e in entries {
        assert!(e["startedDateTime"].is_string());
        assert!(e["timings"].is_object(), "HAR entries require timings");
    }
}
