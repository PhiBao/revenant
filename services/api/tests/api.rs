//! API tests mapped to `.kiro/specs/resurrection-api/requirements.md`.

use std::path::PathBuf;
use std::time::Duration;

use axum::body::Body;
use axum::http::{header, Method, Request, StatusCode};
use http_body_util::BodyExt;
use revenant_api::{router, App, Config};
use serde_json::{json, Value};
use tower::ServiceExt;

fn config() -> Config {
    Config {
        fixtures_dir: PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures"),
        ..Config::default()
    }
}

fn app(cfg: Config) -> axum::Router {
    router(App::new(cfg).expect("app loads"))
}

async fn call(
    app: &axum::Router,
    req: Request<Body>,
) -> (StatusCode, axum::http::HeaderMap, Vec<u8>) {
    let res = app.clone().oneshot(req).await.expect("response");
    let status = res.status();
    let headers = res.headers().clone();
    let body = res
        .into_body()
        .collect()
        .await
        .expect("body")
        .to_bytes()
        .to_vec();
    (status, headers, body)
}

fn get(uri: &str) -> Request<Body> {
    Request::builder().uri(uri).body(Body::empty()).unwrap()
}

fn post(body: &str) -> Request<Body> {
    Request::builder()
        .method(Method::POST)
        .uri("/api/resurrections")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(body.to_string()))
        .unwrap()
}

fn as_json(bytes: &[u8]) -> Value {
    serde_json::from_slice(bytes).expect("json body")
}

async fn create(app: &axum::Router, date: &str, policy: &str) -> Value {
    let body = json!({ "example": "y2k-briefings", "date": date, "policy": policy });
    let (status, _, bytes) = call(app, post(&body.to_string())).await;
    assert_eq!(
        status,
        StatusCode::CREATED,
        "{}",
        String::from_utf8_lossy(&bytes)
    );
    as_json(&bytes)
}

#[tokio::test]
async fn health_is_ok() {
    let (status, _, _) = call(&app(config()), get("/api/health")).await;
    assert_eq!(status, StatusCode::OK);
}

#[tokio::test]
async fn examples_list_the_seeded_fixture_with_its_rights_basis() {
    let (status, _, body) = call(&app(config()), get("/api/examples")).await;
    assert_eq!(status, StatusCode::OK);
    let v = as_json(&body);
    let ex = &v["examples"][0];
    assert_eq!(ex["id"], "y2k-briefings");
    assert_eq!(ex["default_date"], "2000-03-02");
    let rights = ex["rights"].as_str().unwrap();
    assert!(rights.contains("federal"), "{rights}");
    // The line must carry the caveat, not claim certainty (rights review, Nov 11 2026).
    assert!(rights.contains("not confirmed"), "{rights}");
    assert!(!rights.contains("public domain"), "{rights}");
    assert!(
        ex.get("dir").is_none(),
        "internal directory names stay private"
    );
}

#[tokio::test]
async fn create_returns_a_seeded_job_with_certificate_and_urls() {
    let a = app(config());
    let v = create(&a, "2000-03-02", "include-anachronisms").await;
    assert_eq!(v["mode"], "SEEDED");
    assert_eq!(v["id"].as_str().unwrap().len(), 32);
    // The page was replaced at 01:06 on 2000-03-02, so by the end of that day the newer copy
    // is the true one (R6.2).
    let c = &v["certificate"];
    assert_eq!(c["files"][0]["verdict"], "ATTESTED");
    assert_eq!(c["files"][0]["verdict_from_dates_only"], "UNCERTAIN");
    assert_eq!(c["vitals"]["completeness_percent"], 58.3);
    assert_eq!(c["vitals"]["fidelity_percent"], 100.0);
    assert_eq!(c["vitals_from_dates_only"]["fidelity_percent"], 0.0);
    let id = v["id"].as_str().unwrap();
    assert_eq!(v["urls"]["preview"], format!("/p/{id}/"));
}

#[tokio::test]
async fn zip_download_matches_the_reported_hash() {
    use std::process::{Command, Stdio};
    let a = app(config());
    let v = create(&a, "2000-03-02", "include-anachronisms").await;
    let id = v["id"].as_str().unwrap();
    let (status, headers, zip) =
        call(&a, get(&format!("/api/resurrections/{id}/archive.zip"))).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(headers[header::CONTENT_TYPE], "application/zip");
    let disposition = headers[header::CONTENT_DISPOSITION].to_str().unwrap();
    assert!(disposition.starts_with("attachment;"), "{disposition}");
    assert!(
        disposition.contains("revenant-y2k-briefings-2000-03-02-with-anachronisms.zip"),
        "{disposition}"
    );
    // Hash with the system tool so the test does not reuse the code under test.
    let mut child = Command::new("sha256sum")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("sha256sum");
    use std::io::Write;
    child.stdin.take().unwrap().write_all(&zip).unwrap();
    let out = child.wait_with_output().unwrap();
    let hex = String::from_utf8_lossy(&out.stdout);
    assert_eq!(hex.split_whitespace().next().unwrap(), v["zip_sha256"]);
}

#[tokio::test]
async fn the_same_request_gives_the_same_zip_hash_across_jobs() {
    let a = app(config());
    let one = create(&a, "2000-03-02", "strict").await;
    let two = create(&a, "2000-03-02", "strict").await;
    assert_ne!(one["id"], two["id"]);
    assert_eq!(one["zip_sha256"], two["zip_sha256"]);
}

#[tokio::test]
async fn show_returns_the_job_and_unknown_ids_are_404() {
    let a = app(config());
    let v = create(&a, "2000-03-02", "strict").await;
    let id = v["id"].as_str().unwrap();
    let (status, headers, body) = call(&a, get(&format!("/api/resurrections/{id}"))).await;
    assert_eq!(status, StatusCode::OK);
    assert!(headers[header::CONTENT_TYPE]
        .to_str()
        .unwrap()
        .starts_with("application/json"));
    assert_eq!(as_json(&body)["zip_sha256"], v["zip_sha256"]);
    assert_eq!(headers[header::CACHE_CONTROL], "no-store");

    let missing = "0".repeat(32);
    let (status, _, body) = call(&a, get(&format!("/api/resurrections/{missing}"))).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(as_json(&body)["error"]["code"], "not_found");
}

#[tokio::test]
async fn bad_requests_get_400_with_a_code() {
    let a = app(config());
    let cases = [
        (
            r#"{"example":"y2k-briefings","date":"2000-03-02","extra":1}"#,
            "bad_request",
        ),
        (
            r#"{"example":"nope","date":"2000-03-02"}"#,
            "unknown_example",
        ),
        (
            r#"{"example":"y2k-briefings","date":"2000-02-30"}"#,
            "bad_date",
        ),
        (
            r#"{"example":"y2k-briefings","date":"March 2"}"#,
            "bad_date",
        ),
        (
            r#"{"example":"y2k-briefings","date":"2000-03-02","policy":"yolo"}"#,
            "bad_request",
        ),
        ("not json", "bad_request"),
    ];
    for (body, code) in cases {
        let (status, _, bytes) = call(&a, post(body)).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
        assert_eq!(as_json(&bytes)["error"]["code"], code, "{body}");
    }
}

#[tokio::test]
async fn oversize_bodies_get_413() {
    let a = app(config());
    let big = format!(
        r#"{{"example":"{}","date":"2000-03-02"}}"#,
        "x".repeat(20_000)
    );
    let (status, _, bytes) = call(&a, post(&big)).await;
    assert_eq!(status, StatusCode::PAYLOAD_TOO_LARGE);
    assert_eq!(as_json(&bytes)["error"]["code"], "body_too_large");
}

#[tokio::test]
async fn a_date_before_the_page_was_captured_is_422_and_stores_nothing() {
    let a = app(config());
    let body = json!({ "example": "y2k-briefings", "date": "1999-01-01", "policy": "strict" });
    let (status, _, bytes) = call(&a, post(&body.to_string())).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    let v = as_json(&bytes);
    assert_eq!(v["error"]["code"], "no_capture");
    assert_eq!(v["error"]["verdict"], "ANACHRONISM");
    assert!(v["error"]["next_step"].as_str().unwrap().len() > 10);
}

#[tokio::test]
async fn the_rate_limit_answers_429_with_retry_after() {
    let a = app(Config {
        rate_limit: 3,
        ..config()
    });
    for _ in 0..3 {
        create(&a, "2000-03-02", "strict").await;
    }
    let body = json!({ "example": "y2k-briefings", "date": "2000-03-02" });
    let (status, headers, bytes) = call(&a, post(&body.to_string())).await;
    assert_eq!(status, StatusCode::TOO_MANY_REQUESTS);
    assert!(headers.contains_key(header::RETRY_AFTER));
    assert_eq!(as_json(&bytes)["error"]["code"], "rate_limited");
}

#[tokio::test]
async fn jobs_expire() {
    let a = app(Config {
        job_ttl: Duration::ZERO,
        ..config()
    });
    let v = create(&a, "2000-03-02", "strict").await;
    let id = v["id"].as_str().unwrap();
    let (status, _, _) = call(&a, get(&format!("/api/resurrections/{id}"))).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn the_oldest_job_is_evicted_when_the_store_is_full() {
    let a = app(Config {
        max_jobs: 2,
        ..config()
    });
    let first = create(&a, "2000-03-02", "strict").await;
    let _second = create(&a, "2000-03-02", "strict").await;
    let _third = create(&a, "2000-03-02", "strict").await;
    let id = first["id"].as_str().unwrap();
    let (status, _, _) = call(&a, get(&format!("/api/resurrections/{id}"))).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

// ------------------------------------------------------------ preview (A3)

#[tokio::test]
async fn preview_serves_the_page_with_sandboxing_headers() {
    let a = app(config());
    let v = create(&a, "2000-03-02", "include-anachronisms").await;
    let id = v["id"].as_str().unwrap();
    let (status, h, body) = call(&a, get(&format!("/p/{id}/"))).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(h[header::CONTENT_TYPE], "text/html");
    let csp = h[header::CONTENT_SECURITY_POLICY].to_str().unwrap();
    assert!(csp.starts_with("sandbox;"), "{csp}");
    assert!(csp.contains("default-src 'none'"), "{csp}");
    assert_eq!(h[header::X_CONTENT_TYPE_OPTIONS], "nosniff");
    assert_eq!(h[header::REFERRER_POLICY], "no-referrer");
    assert!(h["x-robots-tag"].to_str().unwrap().contains("noindex"));
    assert!(!String::from_utf8_lossy(&body).contains("web.archive.org"));
}

#[tokio::test]
async fn preview_serves_restored_images_and_only_those() {
    let a = app(config());
    let v = create(&a, "2000-03-02", "include-anachronisms").await;
    let id = v["id"].as_str().unwrap();
    let (status, h, body) = call(&a, get(&format!("/p/{id}/images/1.gif"))).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(h[header::CONTENT_TYPE], "image/gif");
    assert!(body.starts_with(b"GIF8"));
    // images/8.gif was never archived, so it is not in the job.
    let (status, _, _) = call(&a, get(&format!("/p/{id}/images/8.gif"))).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn strict_jobs_serve_attested_images_and_never_the_ones_that_were_not_archived() {
    let a = app(config());
    let v = create(&a, "2000-03-02", "strict").await;
    let id = v["id"].as_str().unwrap();
    // The server says images/1.gif had not changed since 1999, so strict keeps it (R6.2).
    let (status, _, body) = call(&a, get(&format!("/p/{id}/images/1.gif"))).await;
    assert_eq!(status, StatusCode::OK);
    assert!(body.starts_with(b"GIF8"));
    // images/8.gif was only ever archived as an error page.
    let (status, _, _) = call(&a, get(&format!("/p/{id}/images/8.gif"))).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn preview_refuses_paths_outside_the_job() {
    let a = app(config());
    let v = create(&a, "2000-03-02", "include-anachronisms").await;
    let id = v["id"].as_str().unwrap();
    for path in [
        "../../../../Cargo.toml",
        "..%2f..%2f..%2fCargo.toml",
        "%2e%2e/%2e%2e/Cargo.toml",
        "images/../../Cargo.toml",
        "/etc/passwd",
        "//etc/passwd",
        "images\\1.gif",
        "images/1.gif%00.html",
    ] {
        let (status, _, body) = call(&a, get(&format!("/p/{id}/{path}"))).await;
        assert!(
            status == StatusCode::NOT_FOUND || status == StatusCode::BAD_REQUEST,
            "{path} gave {status}"
        );
        assert!(
            !String::from_utf8_lossy(&body).contains("[workspace]"),
            "{path}"
        );
    }
}

#[tokio::test]
async fn preview_of_an_unknown_job_is_404_and_a_bare_path_redirects() {
    let a = app(config());
    let (status, _, _) = call(&a, get(&format!("/p/{}/", "0".repeat(32)))).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, _, _) = call(&a, get("/p/not-an-id")).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let v = create(&a, "2000-03-02", "strict").await;
    let id = v["id"].as_str().unwrap();
    let (status, h, _) = call(&a, get(&format!("/p/{id}"))).await;
    assert_eq!(status, StatusCode::PERMANENT_REDIRECT);
    assert_eq!(h[header::LOCATION], format!("/p/{id}/"));
}

#[tokio::test]
async fn the_json_api_never_serves_restored_html() {
    let a = app(config());
    let v = create(&a, "2000-03-02", "include-anachronisms").await;
    let id = v["id"].as_str().unwrap();
    for uri in [
        format!("/api/resurrections/{id}"),
        format!("/api/resurrections/{id}/briefings.html"),
        "/api/examples".to_string(),
    ] {
        let (_, h, _) = call(&a, get(&uri)).await;
        let ct = h
            .get(header::CONTENT_TYPE)
            .map(|v| v.to_str().unwrap())
            .unwrap_or("");
        assert!(!ct.starts_with("text/html"), "{uri} served {ct}");
    }
}

#[tokio::test]
async fn the_certificate_file_is_served_as_json() {
    let a = app(config());
    let v = create(&a, "2000-03-02", "strict").await;
    let id = v["id"].as_str().unwrap();
    let (status, h, body) = call(&a, get(&format!("/p/{id}/REVENANT-CERTIFICATE.json"))).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(h[header::CONTENT_TYPE], "application/json");
    assert_eq!(as_json(&body)["schema"], "revenant.certificate/2");
}

// ------------------------------------------------------------ security review (A1.6, A1.7)

fn post_from(forwarded: &str) -> Request<Body> {
    Request::builder()
        .method(Method::POST)
        .uri("/api/resurrections")
        .header(header::CONTENT_TYPE, "application/json")
        .header("x-forwarded-for", forwarded)
        .body(Body::from(
            r#"{"example":"y2k-briefings","date":"2000-03-02"}"#,
        ))
        .unwrap()
}

#[tokio::test]
async fn a_spoofed_left_most_forwarded_address_does_not_evade_the_rate_limit() {
    let a = app(Config {
        trust_proxy: true,
        rate_limit: 3,
        ..config()
    });
    // Same client as the proxy saw it (last entry), a different fake first entry every time.
    let mut statuses = Vec::new();
    for i in 0..5 {
        let (status, _, _) = call(&a, post_from(&format!("10.0.0.{i}, 203.0.113.9"))).await;
        statuses.push(status);
    }
    assert_eq!(
        statuses,
        [
            StatusCode::CREATED,
            StatusCode::CREATED,
            StatusCode::CREATED,
            StatusCode::TOO_MANY_REQUESTS,
            StatusCode::TOO_MANY_REQUESTS
        ]
    );
}

#[tokio::test]
async fn different_real_clients_behind_the_proxy_have_separate_limits() {
    let a = app(Config {
        trust_proxy: true,
        rate_limit: 1,
        ..config()
    });
    assert_eq!(
        call(&a, post_from("198.51.100.1")).await.0,
        StatusCode::CREATED
    );
    assert_eq!(
        call(&a, post_from("198.51.100.2")).await.0,
        StatusCode::CREATED
    );
    assert_eq!(
        call(&a, post_from("198.51.100.1")).await.0,
        StatusCode::TOO_MANY_REQUESTS
    );
}

#[tokio::test]
async fn forwarded_headers_are_ignored_when_no_proxy_is_trusted() {
    let a = app(Config {
        trust_proxy: false,
        rate_limit: 2,
        ..config()
    });
    // Without a trusted proxy every request shares the socket address, whatever it claims.
    assert_eq!(call(&a, post_from("1.1.1.1")).await.0, StatusCode::CREATED);
    assert_eq!(call(&a, post_from("2.2.2.2")).await.0, StatusCode::CREATED);
    assert_eq!(
        call(&a, post_from("3.3.3.3")).await.0,
        StatusCode::TOO_MANY_REQUESTS
    );
}

#[tokio::test]
async fn retained_jobs_stay_within_the_byte_budget() {
    // One include-anachronisms job is about 70 KB (ZIP plus the files kept for previews).
    let a = app(Config {
        max_job_bytes: 200 * 1024,
        rate_limit: 1000,
        ..config()
    });
    let mut ids = Vec::new();
    for _ in 0..6 {
        let v = create(&a, "2000-03-02", "include-anachronisms").await;
        ids.push(v["id"].as_str().unwrap().to_string());
    }
    let mut alive = 0;
    for id in &ids {
        if call(&a, get(&format!("/api/resurrections/{id}"))).await.0 == StatusCode::OK {
            alive += 1;
        }
    }
    assert!((1..6).contains(&alive), "{alive} of 6 jobs are still held");
    // The newest job is always kept, and the oldest is the first to go.
    let (newest, _, _) = call(&a, get(&format!("/api/resurrections/{}", ids[5]))).await;
    let (oldest, _, _) = call(&a, get(&format!("/api/resurrections/{}", ids[0]))).await;
    assert_eq!(newest, StatusCode::OK);
    assert_eq!(oldest, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn a_bad_frame_ancestors_value_stops_the_server_from_starting() {
    for bad in ["x\ny", "a; sandbox", "bad\u{7f}"] {
        let result = App::new(Config {
            frame_ancestors: bad.to_string(),
            ..config()
        });
        assert!(result.is_err(), "{bad:?} was accepted");
    }
    assert!(App::new(Config {
        frame_ancestors: "'self' https://app.example.org".to_string(),
        ..config()
    })
    .is_ok());
}

// ------------------------------------------------------------ E1: the nearest policy

#[tokio::test]
async fn the_nearest_policy_is_accepted_and_named_in_the_download() {
    let a = app(config());
    let v = create(&a, "2000-03-02", "nearest").await;
    assert_eq!(v["policy"], "nearest");
    assert_eq!(v["certificate"]["policy"], "nearest");
    let id = v["id"].as_str().unwrap();
    let (status, headers, _) = call(&a, get(&format!("/api/resurrections/{id}/archive.zip"))).await;
    assert_eq!(status, StatusCode::OK);
    let disposition = headers[header::CONTENT_DISPOSITION].to_str().unwrap();
    assert!(
        disposition.contains("revenant-y2k-briefings-2000-03-02-nearest.zip"),
        "{disposition}"
    );
}

/// For this page the playback-style restore and Revenant's agree on every file (R7.3), which is
/// what the web app uses to decide whether a side-by-side comparison would show anything.
#[tokio::test]
async fn nearest_and_strict_agree_file_for_file_on_this_specimen() {
    let a = app(config());
    let strict = create(&a, "2000-03-02", "strict").await;
    let near = create(&a, "2000-03-02", "nearest").await;
    let files = |v: &Value| -> Vec<(String, bool, Value)> {
        v["certificate"]["files"]
            .as_array()
            .unwrap()
            .iter()
            .map(|f| {
                (
                    f["path"].as_str().unwrap().to_string(),
                    f["included"].as_bool().unwrap(),
                    f["sha256"].clone(),
                )
            })
            .collect()
    };
    assert_eq!(files(&strict), files(&near));
    assert_ne!(strict["zip_sha256"], near["zip_sha256"]);
}

// ------------------------------------------------------------ E3: the annotated page

#[tokio::test]
async fn the_annotated_page_draws_verdicts_and_marks_the_gaps() {
    let a = app(config());
    let v = create(&a, "2000-03-02", "strict").await;
    let id = v["id"].as_str().unwrap();
    let (status, headers, body) = call(&a, get(&format!("/p/{id}/?annotate=1"))).await;
    assert_eq!(status, StatusCode::OK);
    let html = String::from_utf8_lossy(&body);
    assert_eq!(html.matches("id=\"rv-bar\"").count(), 1);
    // The six attested images are outlined and tagged with the day the server says they last changed.
    assert_eq!(
        html.matches("class=\"rv rv-attested\"").count(),
        6,
        "{html}"
    );
    assert!(html.contains("attested 1999-12-27"));
    // The five files that were never archived are labelled gaps, not broken-image icons.
    assert_eq!(html.matches("class=\"rv-gap rv-missing\"").count(), 5);
    assert!(html.contains("not archived"));
    assert!(!html.contains("web.archive.org"));
    // E3.3: same security headers as any other preview.
    let csp = headers[header::CONTENT_SECURITY_POLICY].to_str().unwrap();
    assert!(csp.starts_with("sandbox;"), "{csp}");
    assert_eq!(headers[header::X_CONTENT_TYPE_OPTIONS], "nosniff");
}

#[tokio::test]
async fn without_the_parameter_the_page_is_exactly_what_is_in_the_zip() {
    let a = app(config());
    let v = create(&a, "2000-03-02", "strict").await;
    let id = v["id"].as_str().unwrap();
    let (_, _, plain) = call(&a, get(&format!("/p/{id}/"))).await;
    let text = String::from_utf8_lossy(&plain);
    assert!(!text.contains("rv-bar") && !text.contains("data-rv-tag") && !text.contains("rv-gap"));
    // E3.2: asking for the annotated page changes nothing about the stored page or the ZIP.
    let _ = call(&a, get(&format!("/p/{id}/?annotate=1"))).await;
    let (_, _, again) = call(&a, get(&format!("/p/{id}/"))).await;
    assert_eq!(plain, again);
    let (_, _, zip) = call(&a, get(&format!("/api/resurrections/{id}/archive.zip"))).await;
    let mut hasher = std::process::Command::new("sha256sum")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    use std::io::Write;
    hasher.stdin.take().unwrap().write_all(&zip).unwrap();
    let out = hasher.wait_with_output().unwrap();
    assert_eq!(
        String::from_utf8_lossy(&out.stdout)
            .split_whitespace()
            .next()
            .unwrap(),
        v["zip_sha256"]
    );
}

#[tokio::test]
async fn the_annotate_parameter_does_not_reach_other_files() {
    let a = app(config());
    let v = create(&a, "2000-03-02", "strict").await;
    let id = v["id"].as_str().unwrap();
    let (_, _, plain) = call(&a, get(&format!("/p/{id}/images/1.gif"))).await;
    let (status, _, with) = call(&a, get(&format!("/p/{id}/images/1.gif?annotate=1"))).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(plain, with);
}

#[tokio::test]
async fn other_values_of_the_annotate_parameter_give_the_plain_page() {
    let a = app(config());
    let v = create(&a, "2000-03-02", "strict").await;
    let id = v["id"].as_str().unwrap();
    for q in [
        "annotate=0",
        "annotate=true",
        "annotate=",
        "annotate=1%00",
        "annotate[]=1",
    ] {
        let (status, _, body) = call(&a, get(&format!("/p/{id}/?{q}"))).await;
        assert_eq!(status, StatusCode::OK, "{q}");
        assert!(!String::from_utf8_lossy(&body).contains("rv-bar"), "{q}");
    }
}

// ------------------------------------------------------------------ A5: dates-only (R10)

#[tokio::test]
async fn dates_only_ignores_server_dates_and_says_so() {
    let a = app(config());
    let v = create(&a, "2000-03-02", "dates-only").await;
    assert_eq!(v["policy"], "dates-only");
    let c = &v["certificate"];
    assert_eq!(c["policy"], "dates-only");
    for f in c["files"].as_array().unwrap() {
        assert_ne!(f["verdict"], "ATTESTED", "{f}");
        assert!(f["origin_modified"].is_null(), "{f}");
    }
    // What it scores is exactly what the strict restore reports as "from dates alone" (R10.1).
    let strict = create(&a, "2000-03-02", "strict").await;
    assert_eq!(
        c["vitals"]["fidelity_percent"],
        strict["certificate"]["vitals_from_dates_only"]["fidelity_percent"]
    );
    assert!(c["notes"]
        .as_array()
        .unwrap()
        .iter()
        .any(|n| n.as_str().unwrap().contains("Server dates were not used")));
}

#[tokio::test]
async fn dates_only_never_contains_a_file_the_strict_restore_lacks() {
    let a = app(config());
    for date in ["2000-03-01", "2000-03-02", "2000-03-05"] {
        let d = create(&a, date, "dates-only").await;
        let s = create(&a, date, "strict").await;
        let included = |v: &Value| -> Vec<String> {
            v["certificate"]["files"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|f| f["included"] == true)
                .map(|f| f["path"].as_str().unwrap().to_string())
                .collect()
        };
        let strict_files = included(&s);
        for path in included(&d) {
            assert!(strict_files.contains(&path), "{date}: {path}");
        }
    }
}

#[tokio::test]
async fn the_server_dates_rescue_files_on_the_specimen() {
    let a = app(config());
    let d = create(&a, "2000-03-02", "dates-only").await;
    let s = create(&a, "2000-03-02", "strict").await;
    let count = |v: &Value| {
        v["certificate"]["files"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|f| f["included"] == true)
            .count()
    };
    // Six images were captured only after the day. Dates alone leave them out.
    assert_eq!(count(&s) - count(&d), 6);
}
