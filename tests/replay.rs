//! End-to-end replay tests: capture against a local server, then replay from
//! the session database and confirm the upstream receives identical requests.

mod support;
use support::*;

fn request_id(list: &str, needle: &str) -> String {
    let line = list
        .lines()
        .find(|l| l.contains(needle) && l.contains('['))
        .unwrap_or_else(|| panic!("no line with {needle}:\n{list}"));
    let start = line.rfind('[').unwrap() + 1;
    let end = line.rfind(']').unwrap();
    line[start..end].to_string()
}

#[tokio::test(flavor = "multi_thread")]
async fn replay_by_id_resends_method_path_and_body() {
    let (upstream, seen) = start_http_upstream().await;
    let env = Env::new();
    let body = r#"{"order":42,"qty":3}"#;
    let list = {
        let proxy = env.capture("rp");
        let r = http_via_proxy(proxy.addr, "POST", upstream, "/orders?src=test", Some(body)).await;
        assert_eq!(r.status, 200);
        env.wait_list("rp", |t| t.contains("POST /orders"))
    };
    let id = request_id(&list, "POST /orders");

    // Replay runs without the proxy: straight from SQLite to the upstream.
    env.run(&["replay", "--session", "rp", "--id", &id, "--count", "2"]);

    let seen = seen.lock().unwrap();
    assert_eq!(seen.len(), 3, "1 captured + 2 replays: {seen:?}");
    for s in seen.iter() {
        assert_eq!(s.method, "POST");
        assert_eq!(s.path, "/orders?src=test");
        assert_eq!(s.body, body);
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn replay_dry_run_sends_nothing() {
    let (upstream, seen) = start_http_upstream().await;
    let env = Env::new();
    let list = {
        let proxy = env.capture("dry");
        http_via_proxy(proxy.addr, "DELETE", upstream, "/danger", None).await;
        env.wait_list("dry", |t| t.contains("DELETE /danger"))
    };
    let id = request_id(&list, "DELETE /danger");
    env.run(&["replay", "--session", "dry", "--id", &id, "--dry-run"]);
    assert_eq!(
        seen.lock().unwrap().len(),
        1,
        "dry run must not hit upstream"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn replay_filtered_only_matching_requests() {
    let (upstream, seen) = start_http_upstream().await;
    let env = Env::new();
    {
        let proxy = env.capture("flt");
        http_via_proxy(proxy.addr, "GET", upstream, "/a", None).await;
        http_via_proxy(proxy.addr, "POST", upstream, "/b", Some("{}")).await;
        http_via_proxy(proxy.addr, "GET", upstream, "/c", None).await;
        env.wait_list("flt", |t| {
            t.contains("/a") && t.contains("/b") && t.contains("/c")
        });
    }
    env.run(&["replay", "--session", "flt", "--filter", "method=POST"]);
    let seen = seen.lock().unwrap();
    assert_eq!(seen.len(), 4);
    assert_eq!(seen[3].method, "POST");
    assert_eq!(seen[3].path, "/b");
}

#[tokio::test(flavor = "multi_thread")]
async fn replay_with_diff_reports_changed_response() {
    let (upstream, _) = start_http_upstream().await;
    let env = Env::new();
    let list = {
        let proxy = env.capture("diff");
        http_via_proxy(proxy.addr, "GET", upstream, "/counter", None).await;
        env.wait_list("diff", |t| t.contains("GET /counter"))
    };
    let id = request_id(&list, "GET /counter");
    // The echo server's "n" field increments, so the replayed body differs.
    let out = env.run(&["replay", "--session", "diff", "--id", &id, "--diff"]);
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(text.contains("diff"), "expected diff output:\n{text}");
    assert!(
        text.contains("n"),
        "expected changed field in diff:\n{text}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn replay_unknown_id_fails() {
    let env = Env::new();
    let out = env
        .command()
        .args(["replay", "--session", "none", "--id", "does-not-exist"])
        .output()
        .unwrap();
    assert!(!out.status.success(), "unknown id should be an error");
}
