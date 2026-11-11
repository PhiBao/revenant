//! Live-mode tests (requirement group L). A local fake archive serves the real y2k-briefings
//! fixture over the archive's own URL shapes and can misbehave on demand, so these tests never
//! touch the real archive.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use axum::body::Body;
use axum::extract::{Path, Query, State};
use axum::http::{header, Method, Request, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::Router;
use http_body_util::BodyExt;
use revenant_api::{router, App, Config};
use serde_json::{json, Value};
use tower::ServiceExt;

// ------------------------------------------------------------------ fake archive

#[derive(Default, Clone)]
struct Behavior {
    /// Answer the first CDX request with 429 and `Retry-After: 0`.
    rate_limit_first_cdx: bool,
    /// Answer every CDX request with this status.
    cdx_status: Option<u16>,
    /// Redirect every replay to another host.
    redirect_replay_off_host: bool,
    /// Serve a replay body larger than the 4 MiB limit.
    huge_replay: bool,
    /// Delay every CDX answer.
    cdx_delay_ms: u64,
}

struct Row {
    label: String,
    ts: String,
    digest: String,
    status: String,
    original: String,
}

struct Fake {
    rows: Vec<Row>,
    /// (label, ts) -> bytes
    blobs: HashMap<(String, String), Vec<u8>>,
    behavior: Behavior,
    log: Mutex<Vec<String>>,
    cdx_calls: AtomicU32,
    replay_calls: AtomicU32,
}

fn fixture_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/snapshots/y2k-briefings")
}

fn load_fake(behavior: Behavior) -> Arc<Fake> {
    let dir = fixture_dir();
    let rows = std::fs::read_to_string(dir.join("captures.tsv"))
        .unwrap()
        .lines()
        .skip(1)
        .filter(|l| !l.trim().is_empty())
        .map(|l| {
            let c: Vec<&str> = l.split('\t').collect();
            Row {
                label: c[0].into(),
                ts: c[1].into(),
                digest: c[2].into(),
                status: c[4].into(),
                original: c[5].into(),
            }
        })
        .collect();
    let mut blobs = HashMap::new();
    for line in std::fs::read_to_string(dir.join("blobs.tsv"))
        .unwrap()
        .lines()
        .skip(1)
    {
        let c: Vec<&str> = line.split('\t').collect();
        blobs.insert(
            (c[0].to_string(), c[1].to_string()),
            std::fs::read(dir.join("blobs").join(c[3])).unwrap(),
        );
    }
    Arc::new(Fake {
        rows,
        blobs,
        behavior,
        log: Mutex::new(Vec::new()),
        cdx_calls: AtomicU32::new(0),
        replay_calls: AtomicU32::new(0),
    })
}

async fn fake_cdx(
    State(f): State<Arc<Fake>>,
    Query(q): Query<HashMap<String, String>>,
) -> Response {
    let n = f.cdx_calls.fetch_add(1, Ordering::SeqCst);
    f.log.lock().unwrap().push(format!(
        "cdx url={}",
        q.get("url").cloned().unwrap_or_default()
    ));
    if f.behavior.cdx_delay_ms > 0 {
        tokio::time::sleep(Duration::from_millis(f.behavior.cdx_delay_ms)).await;
    }
    if f.behavior.rate_limit_first_cdx && n == 0 {
        return (
            StatusCode::TOO_MANY_REQUESTS,
            [(header::RETRY_AFTER, "0")],
            "slow down",
        )
            .into_response();
    }
    if let Some(code) = f.behavior.cdx_status {
        return (StatusCode::from_u16(code).unwrap(), "no").into_response();
    }
    let label = q.get("url").cloned().unwrap_or_default();
    let mut hits: Vec<&Row> = f
        .rows
        .iter()
        .filter(|r| r.label == label && r.status == "200")
        .filter(|r| q.get("to").is_none_or(|to| r.ts.as_str() <= to.as_str()))
        .filter(|r| {
            q.get("from")
                .is_none_or(|from| r.ts.as_str() >= from.as_str())
        })
        .collect();
    hits.sort_by(|a, b| a.ts.cmp(&b.ts));
    let limit: i64 = q.get("limit").and_then(|l| l.parse().ok()).unwrap_or(0);
    let hits: Vec<&Row> = match limit {
        l if l < 0 => hits
            .iter()
            .rev()
            .take(l.unsigned_abs() as usize)
            .rev()
            .copied()
            .collect(),
        l if l > 0 => hits.into_iter().take(l as usize).collect(),
        _ => hits,
    };
    let mut out = vec![json!(["original", "timestamp", "digest"])];
    out.extend(hits.iter().map(|r| json!([r.original, r.ts, r.digest])));
    if out.len() == 1 {
        return "[]".into_response();
    }
    Body::from(Value::Array(out).to_string()).into_response()
}

async fn fake_replay(State(f): State<Arc<Fake>>, Path(rest): Path<String>) -> Response {
    f.replay_calls.fetch_add(1, Ordering::SeqCst);
    f.log.lock().unwrap().push(format!("replay {rest}"));
    if f.behavior.redirect_replay_off_host {
        return (
            StatusCode::FOUND,
            [(header::LOCATION, "http://evil.example/steal")],
            "",
        )
            .into_response();
    }
    if f.behavior.huge_replay {
        return vec![b'x'; 5 * 1024 * 1024].into_response();
    }
    // rest = "<ts>id_/<original>"
    let Some((stamp, original)) = rest.split_once("id_/") else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let Some(row) = f
        .rows
        .iter()
        .find(|r| r.ts == stamp && r.original == original)
    else {
        return StatusCode::NOT_FOUND.into_response();
    };
    match f.blobs.get(&(row.label.clone(), row.ts.clone())) {
        Some(bytes) => bytes.clone().into_response(),
        None => StatusCode::NOT_FOUND.into_response(),
    }
}

async fn spawn_fake(behavior: Behavior) -> (String, Arc<Fake>) {
    let fake = load_fake(behavior);
    let app = Router::new()
        .route("/cdx/search/cdx", get(fake_cdx))
        .route("/web/{*rest}", get(fake_replay))
        .with_state(fake.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    (format!("http://{addr}"), fake)
}

// ------------------------------------------------------------------ helpers

fn config(archive_base: &str) -> Config {
    Config {
        fixtures_dir: PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures"),
        archive_base: archive_base.to_string(),
        archive_gap: Duration::ZERO,
        ..Config::default()
    }
}

fn app_for(cfg: Config) -> axum::Router {
    router(App::new(cfg).expect("app loads"))
}

async fn call(
    app: &axum::Router,
    req: Request<Body>,
) -> (StatusCode, axum::http::HeaderMap, Vec<u8>) {
    let res = app.clone().oneshot(req).await.expect("response");
    let (status, headers) = (res.status(), res.headers().clone());
    let body = res.into_body().collect().await.unwrap().to_bytes().to_vec();
    (status, headers, body)
}

fn post_live(body: Value) -> Request<Body> {
    Request::builder()
        .method(Method::POST)
        .uri("/api/live")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(body.to_string()))
        .unwrap()
}

fn get_req(uri: &str) -> Request<Body> {
    Request::builder().uri(uri).body(Body::empty()).unwrap()
}

fn good(url: &str, date: &str) -> Value {
    json!({ "url": url, "date": date, "rights": true })
}

fn as_json(b: &[u8]) -> Value {
    serde_json::from_slice(b).expect("json")
}

/// Poll a run until it leaves `running`.
async fn finish(app: &axum::Router, id: &str) -> Value {
    for _ in 0..200 {
        let (status, _, body) = call(app, get_req(&format!("/api/live/{id}"))).await;
        assert_eq!(status, StatusCode::OK);
        let v = as_json(&body);
        if v["status"] != "running" {
            return v;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!("run did not finish");
}

async fn start(app: &axum::Router, body: Value) -> String {
    let (status, _, bytes) = call(app, post_live(body)).await;
    assert_eq!(
        status,
        StatusCode::ACCEPTED,
        "{}",
        String::from_utf8_lossy(&bytes)
    );
    as_json(&bytes)["id"].as_str().unwrap().to_string()
}

const PAGE: &str = "http://www.y2k.gov/briefings.html";

// ------------------------------------------------------------------ L1: input

#[tokio::test(flavor = "multi_thread")]
async fn rights_must_be_confirmed() {
    let (base, fake) = spawn_fake(Behavior::default()).await;
    let app = app_for(config(&base));
    for body in [
        json!({ "url": PAGE, "date": "2000-03-02" }),
        json!({ "url": PAGE, "date": "2000-03-02", "rights": false }),
    ] {
        let (status, _, bytes) = call(&app, post_live(body)).await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(as_json(&bytes)["error"]["code"], "rights_required");
    }
    assert_eq!(
        fake.cdx_calls.load(Ordering::SeqCst),
        0,
        "nothing was started"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn bad_urls_are_refused_before_any_archive_request() {
    let (base, fake) = spawn_fake(Behavior::default()).await;
    let app = app_for(Config {
        live_rate_limit: 100,
        ..config(&base)
    });
    for url in [
        "http://127.0.0.1/",
        "http://localhost/x.html",
        "http://169.254.169.254/latest/meta-data",
        "http://[::1]/",
        "file:///etc/passwd",
        "ftp://example.org/x",
        "http://user:pw@example.org/x.html",
        "http://example.org/../etc/passwd",
        "http://intranet/x.html",
    ] {
        let (status, _, bytes) = call(&app, post_live(good(url, "2000-03-02"))).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{url}");
        assert_eq!(as_json(&bytes)["error"]["code"], "bad_url", "{url}");
    }
    assert_eq!(fake.cdx_calls.load(Ordering::SeqCst), 0);
    assert_eq!(fake.replay_calls.load(Ordering::SeqCst), 0);
}

#[tokio::test(flavor = "multi_thread")]
async fn bad_dates_and_unknown_fields_are_400() {
    let (base, _) = spawn_fake(Behavior::default()).await;
    let app = app_for(Config {
        live_rate_limit: 100,
        ..config(&base)
    });
    let (s, _, b) = call(&app, post_live(good(PAGE, "March 2"))).await;
    assert_eq!(
        (s, as_json(&b)["error"]["code"].clone()),
        (StatusCode::BAD_REQUEST, json!("bad_date"))
    );
    let (s, _, b) = call(
        &app,
        post_live(json!({ "url": PAGE, "date": "2000-03-02", "rights": true, "x": 1 })),
    )
    .await;
    assert_eq!(
        (s, as_json(&b)["error"]["code"].clone()),
        (StatusCode::BAD_REQUEST, json!("bad_request"))
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn the_typed_url_is_never_fetched_only_queried() {
    let (base, fake) = spawn_fake(Behavior::default()).await;
    let app = app_for(config(&base));
    let id = start(
        &app,
        good("http://evil.example/page.html?token=abc#frag", "2000-03-02"),
    )
    .await;
    let v = finish(&app, &id).await;
    // Nothing for evil.example is in the archive, so the run fails honestly, not with a fetch.
    assert_eq!(v["status"], "failed");
    let log = fake.log.lock().unwrap().clone();
    assert!(!log.is_empty());
    for line in &log {
        assert!(
            line.starts_with("cdx url=evil.example/page.html") || line.starts_with("replay "),
            "unexpected archive request: {line}"
        );
        assert!(
            !line.contains("token"),
            "the query string leaked into a request: {line}"
        );
    }
    assert!(v["notes"][0].as_str().unwrap().contains("ignored"));
}

// ------------------------------------------------------------------ L2 and L4: results

#[tokio::test(flavor = "multi_thread")]
async fn a_live_run_matches_the_seeded_result_and_is_labelled_live() {
    let (base, _) = spawn_fake(Behavior::default()).await;
    let app = app_for(config(&base));
    let id = start(&app, good(PAGE, "2000-03-02")).await;
    let v = finish(&app, &id).await;
    assert_eq!(v["status"], "done", "{v}");
    for key in ["strict", "nearest"] {
        assert_eq!(v[key]["mode"], "LIVE");
        assert_eq!(v[key]["certificate"]["mode"], "LIVE");
    }
    let strict = &v["strict"]["certificate"];
    assert_eq!(strict["files"][0]["verdict"], "UNCERTAIN");
    assert_eq!(strict["vitals"]["completeness_percent"], 58.3);
    assert_eq!(strict["vitals"]["fidelity_percent"], 0.0);
    let anachronisms = strict["files"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|f| f["verdict"] == "ANACHRONISM")
        .count();
    assert_eq!(anachronisms, 6);

    // The nearest-capture job holds the six images, and its preview serves them.
    let preview = v["nearest"]["urls"]["preview"]
        .as_str()
        .unwrap()
        .to_string();
    let (s, h, body) = call(&app, get_req(&format!("{preview}images/1.gif"))).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(h[header::CONTENT_TYPE], "image/gif");
    assert!(body.starts_with(b"GIF8"));
    let (s, _, page) = call(&app, get_req(&preview)).await;
    assert_eq!(s, StatusCode::OK);
    assert!(!String::from_utf8_lossy(&page).contains("web.archive.org"));
}

#[tokio::test(flavor = "multi_thread")]
async fn one_run_costs_the_archive_one_set_of_requests() {
    let (base, fake) = spawn_fake(Behavior::default()).await;
    let app = app_for(config(&base));
    let id = start(&app, good(PAGE, "2000-03-02")).await;
    assert_eq!(finish(&app, &id).await["status"], "done");
    // The page and its 11 assets each need a "before" and an "after" query, once.
    assert_eq!(fake.cdx_calls.load(Ordering::SeqCst), 2 * (1 + 11));
    // The page body once, plus the six images the nearest restore includes.
    assert_eq!(fake.replay_calls.load(Ordering::SeqCst), 1 + 6);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_date_before_the_first_capture_is_no_capture() {
    let (base, _) = spawn_fake(Behavior::default()).await;
    let app = app_for(config(&base));
    let id = start(&app, good(PAGE, "1999-01-01")).await;
    let v = finish(&app, &id).await;
    assert_eq!(v["status"], "failed");
    assert_eq!(v["error"]["code"], "no_capture");
}

#[tokio::test(flavor = "multi_thread")]
async fn an_archive_failure_is_not_reported_as_no_capture() {
    for code in [403u16, 500] {
        let (base, _) = spawn_fake(Behavior {
            cdx_status: Some(code),
            ..Behavior::default()
        })
        .await;
        let app = app_for(config(&base));
        let id = start(&app, good(PAGE, "2000-03-02")).await;
        let v = finish(&app, &id).await;
        assert_eq!(v["status"], "failed", "HTTP {code}");
        assert_eq!(v["error"]["code"], "archive_unavailable", "HTTP {code}");
    }
}

// ------------------------------------------------------------------ L3: politeness and limits

#[tokio::test(flavor = "multi_thread")]
async fn a_rate_limited_archive_is_retried_once() {
    let (base, fake) = spawn_fake(Behavior {
        rate_limit_first_cdx: true,
        ..Behavior::default()
    })
    .await;
    let app = app_for(config(&base));
    let id = start(&app, good(PAGE, "2000-03-02")).await;
    assert_eq!(finish(&app, &id).await["status"], "done");
    // One extra request for the retry.
    assert_eq!(fake.cdx_calls.load(Ordering::SeqCst), 2 * (1 + 11) + 1);
}

#[tokio::test(flavor = "multi_thread")]
async fn redirects_off_the_archive_host_are_refused() {
    let (base, _) = spawn_fake(Behavior {
        redirect_replay_off_host: true,
        ..Behavior::default()
    })
    .await;
    let app = app_for(config(&base));
    let id = start(&app, good(PAGE, "2000-03-02")).await;
    let v = finish(&app, &id).await;
    assert_eq!(v["status"], "failed");
    assert_eq!(v["error"]["code"], "archive_unavailable");
    assert!(v["error"]["message"]
        .as_str()
        .unwrap()
        .contains("another host"));
}

#[tokio::test(flavor = "multi_thread")]
async fn an_oversize_body_is_cut_off() {
    let (base, _) = spawn_fake(Behavior {
        huge_replay: true,
        ..Behavior::default()
    })
    .await;
    let app = app_for(config(&base));
    let id = start(&app, good(PAGE, "2000-03-02")).await;
    let v = finish(&app, &id).await;
    assert_eq!(v["status"], "failed");
    assert_eq!(v["error"]["code"], "archive_unavailable");
}

#[tokio::test(flavor = "multi_thread")]
async fn only_so_many_live_runs_execute_at_once() {
    let (base, _) = spawn_fake(Behavior {
        cdx_delay_ms: 250,
        ..Behavior::default()
    })
    .await;
    let app = app_for(Config {
        live_max_concurrent: 1,
        ..config(&base)
    });
    let first = start(&app, good(PAGE, "2000-03-02")).await;

    let (status, headers, bytes) = call(&app, post_live(good(PAGE, "2000-03-02"))).await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(as_json(&bytes)["error"]["code"], "busy");
    assert!(headers.contains_key(header::RETRY_AFTER));

    // The slot comes back when the first run ends.
    assert_eq!(finish(&app, &first).await["status"], "done");
    let again = start(&app, good(PAGE, "2000-03-02")).await;
    assert_eq!(finish(&app, &again).await["status"], "done");
}

#[tokio::test(flavor = "multi_thread")]
async fn live_runs_have_their_own_rate_limit() {
    let (base, _) = spawn_fake(Behavior::default()).await;
    let app = app_for(Config {
        live_rate_limit: 2,
        ..config(&base)
    });
    for _ in 0..2 {
        let id = start(&app, good(PAGE, "2000-03-02")).await;
        finish(&app, &id).await;
    }
    let (status, headers, bytes) = call(&app, post_live(good(PAGE, "2000-03-02"))).await;
    assert_eq!(status, StatusCode::TOO_MANY_REQUESTS);
    assert!(headers.contains_key(header::RETRY_AFTER));
    assert_eq!(as_json(&bytes)["error"]["code"], "rate_limited");
}

#[tokio::test(flavor = "multi_thread")]
async fn unknown_runs_are_404() {
    let (base, _) = spawn_fake(Behavior::default()).await;
    let app = app_for(config(&base));
    let (status, _, _) = call(&app, get_req(&format!("/api/live/{}", "0".repeat(32)))).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test(flavor = "multi_thread")]
async fn seeded_results_stay_labelled_seeded() {
    let (base, _) = spawn_fake(Behavior::default()).await;
    let app = app_for(config(&base));
    let body = json!({ "example": "y2k-briefings", "date": "2000-03-02" });
    let req = Request::builder()
        .method(Method::POST)
        .uri("/api/resurrections")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(body.to_string()))
        .unwrap();
    let (status, _, bytes) = call(&app, req).await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(as_json(&bytes)["mode"], "SEEDED");
}
