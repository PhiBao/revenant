//! Revenant HTTP API. See `.kiro/specs/resurrection-api/requirements.md`.
//!
//! JSON lives under `/api`. Restored files are served only under `/p`, with a sandboxing CSP.

use std::collections::{HashMap, VecDeque};
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use axum::body::{Body, Bytes};
use axum::extract::rejection::BytesRejection;
use axum::extract::{ConnectInfo, DefaultBodyLimit, Extension, Path, Request, State};
use axum::http::{header, HeaderValue, StatusCode};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Redirect, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use revenant_core::pipeline::{
    resurrect, PipelineError, Request as PipelineRequest, Resurrection, SeededSource,
};
use revenant_core::time::parse_cdx_timestamp;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

pub const MAX_BODY_BYTES: usize = 16 * 1024;

/// Runtime settings. Tests override these.
#[derive(Debug, Clone)]
pub struct Config {
    pub fixtures_dir: PathBuf,
    pub web_dir: Option<PathBuf>,
    pub job_ttl: Duration,
    pub max_jobs: usize,
    pub rate_limit: u32,
    pub rate_window: Duration,
    /// Read the client address from `X-Forwarded-For`. Only enable behind a trusted proxy.
    pub trust_proxy: bool,
    pub frame_ancestors: String,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            fixtures_dir: PathBuf::from("fixtures"),
            web_dir: None,
            job_ttl: Duration::from_secs(3600),
            max_jobs: 500,
            rate_limit: 30,
            rate_window: Duration::from_secs(60),
            trust_proxy: false,
            frame_ancestors: "'self'".to_string(),
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AlternateDate {
    pub date: String,
    pub label: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ExampleInfo {
    pub id: String,
    pub title: String,
    #[serde(skip_serializing)]
    pub dir: String,
    pub site: String,
    pub page: String,
    pub default_date: String,
    pub alternate_dates: Vec<AlternateDate>,
    pub blurb: String,
    pub rights: String,
}

struct Example {
    info: ExampleInfo,
    source: Arc<SeededSource>,
}

struct Job {
    example_id: String,
    date: String,
    /// `strict` or `with-anachronisms`, used in the download file name.
    policy: &'static str,
    created: Instant,
    result: Resurrection,
}

#[derive(Default)]
struct Store {
    map: HashMap<String, Arc<Job>>,
    order: VecDeque<String>,
}

pub struct App {
    cfg: Config,
    examples: Vec<Example>,
    jobs: Mutex<Store>,
    hits: Mutex<HashMap<String, (Instant, u32)>>,
}

#[derive(Clone)]
struct ClientKey(String);

impl App {
    /// Load the examples manifest and every fixture it names.
    pub fn new(cfg: Config) -> Result<Arc<Self>, String> {
        let manifest = std::fs::read_to_string(cfg.fixtures_dir.join("examples.json"))
            .map_err(|e| format!("examples.json: {e}"))?;
        let infos: Vec<ExampleInfo> =
            serde_json::from_str(&manifest).map_err(|e| format!("examples.json: {e}"))?;
        let mut examples = Vec::new();
        for info in infos {
            let dir = cfg.fixtures_dir.join("snapshots").join(&info.dir);
            let source = SeededSource::from_dir(&format!("fixture:{}", info.dir), &dir)
                .map_err(|e| format!("fixture {}: {e}", info.id))?;
            examples.push(Example {
                info,
                source: Arc::new(source),
            });
        }
        Ok(Arc::new(Self {
            cfg,
            examples,
            jobs: Mutex::new(Store::default()),
            hits: Mutex::new(HashMap::new()),
        }))
    }

    fn example(&self, id: &str) -> Option<&Example> {
        self.examples.iter().find(|e| e.info.id == id)
    }

    fn insert(&self, id: String, job: Job) {
        let mut store = self.jobs.lock().expect("job store lock");
        let ttl = self.cfg.job_ttl;
        // Drop expired jobs, then make room.
        let expired: Vec<String> = store
            .map
            .iter()
            .filter(|(_, j)| j.created.elapsed() >= ttl)
            .map(|(k, _)| k.clone())
            .collect();
        for k in expired {
            store.map.remove(&k);
            store.order.retain(|o| o != &k);
        }
        while store.map.len() >= self.cfg.max_jobs {
            match store.order.pop_front() {
                Some(old) => {
                    store.map.remove(&old);
                }
                None => break,
            }
        }
        store.order.push_back(id.clone());
        store.map.insert(id, Arc::new(job));
    }

    fn job(&self, id: &str) -> Option<Arc<Job>> {
        let mut store = self.jobs.lock().expect("job store lock");
        let job = store.map.get(id)?.clone();
        if job.created.elapsed() >= self.cfg.job_ttl {
            store.map.remove(id);
            store.order.retain(|o| o != id);
            return None;
        }
        Some(job)
    }

    /// Returns the seconds to wait when the client is over its limit.
    fn rate_check(&self, key: &str) -> Option<u64> {
        let mut hits = self.hits.lock().expect("rate lock");
        let now = Instant::now();
        if hits.len() > 10_000 {
            let window = self.cfg.rate_window;
            hits.retain(|_, (start, _)| now.duration_since(*start) < window);
        }
        let entry = hits.entry(key.to_string()).or_insert((now, 0));
        if now.duration_since(entry.0) >= self.cfg.rate_window {
            *entry = (now, 0);
        }
        entry.1 += 1;
        if entry.1 > self.cfg.rate_limit {
            let wait = self
                .cfg
                .rate_window
                .saturating_sub(now.duration_since(entry.0));
            return Some(wait.as_secs().max(1));
        }
        None
    }
}

// ---------------------------------------------------------------- errors

pub struct ApiError {
    status: StatusCode,
    code: &'static str,
    message: String,
    extra: Option<Value>,
    retry_after: Option<u64>,
}

impl ApiError {
    fn new(status: StatusCode, code: &'static str, message: impl Into<String>) -> Self {
        Self {
            status,
            code,
            message: message.into(),
            extra: None,
            retry_after: None,
        }
    }
    fn bad(code: &'static str, message: impl Into<String>) -> Self {
        Self::new(StatusCode::BAD_REQUEST, code, message)
    }
    fn not_found() -> Self {
        Self::new(
            StatusCode::NOT_FOUND,
            "not_found",
            "Nothing here, or it has expired.",
        )
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let mut error = json!({ "code": self.code, "message": self.message });
        if let (Some(extra), Some(obj)) = (self.extra, error.as_object_mut()) {
            if let Some(more) = extra.as_object() {
                for (k, v) in more {
                    obj.insert(k.clone(), v.clone());
                }
            }
        }
        let mut res = (self.status, Json(json!({ "error": error }))).into_response();
        if let Some(secs) = self.retry_after {
            if let Ok(v) = HeaderValue::from_str(&secs.to_string()) {
                res.headers_mut().insert(header::RETRY_AFTER, v);
            }
        }
        res
    }
}

// ---------------------------------------------------------------- router

pub fn router(app: Arc<App>) -> Router {
    let api = Router::new()
        .route("/api/health", get(health))
        .route("/api/examples", get(examples))
        .route(
            "/api/resurrections",
            post(create).layer(DefaultBodyLimit::max(MAX_BODY_BYTES)),
        )
        .route("/api/resurrections/{id}", get(show))
        .route("/api/resurrections/{id}/archive.zip", get(download))
        .route("/p/{id}", get(preview_redirect))
        .route("/p/{id}/", get(preview_page))
        .route("/p/{id}/{*path}", get(preview_file));

    let mut router = api;
    if let Some(dir) = app.cfg.web_dir.clone() {
        router = router.fallback_service(tower_http::services::ServeDir::new(dir));
    }

    router
        .layer(middleware::from_fn_with_state(app.clone(), client_key))
        .layer(middleware::from_fn(security_headers))
        .with_state(app)
}

async fn client_key(State(app): State<Arc<App>>, mut req: Request, next: Next) -> Response {
    let forwarded = if app.cfg.trust_proxy {
        req.headers()
            .get("x-forwarded-for")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.split(',').next())
            .map(|s| s.trim().to_string())
    } else {
        None
    };
    let key = forwarded
        .or_else(|| {
            req.extensions()
                .get::<ConnectInfo<SocketAddr>>()
                .map(|c| c.0.ip().to_string())
        })
        .unwrap_or_else(|| "unknown".to_string());
    req.extensions_mut().insert(ClientKey(key));
    next.run(req).await
}

async fn security_headers(req: Request, next: Next) -> Response {
    let is_api = req.uri().path().starts_with("/api");
    let mut res = next.run(req).await;
    let h = res.headers_mut();
    h.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    h.insert(
        header::REFERRER_POLICY,
        HeaderValue::from_static("no-referrer"),
    );
    if is_api {
        h.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    }
    res
}

// ---------------------------------------------------------------- handlers

async fn health() -> Json<Value> {
    Json(json!({ "status": "ok" }))
}

async fn examples(State(app): State<Arc<App>>) -> Json<Value> {
    let list: Vec<&ExampleInfo> = app.examples.iter().map(|e| &e.info).collect();
    Json(json!({ "examples": list }))
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct CreateBody {
    example: String,
    date: String,
    #[serde(default)]
    policy: PolicyField,
}

#[derive(Debug, Deserialize, Default, Clone, Copy)]
enum PolicyField {
    #[default]
    #[serde(rename = "strict")]
    Strict,
    #[serde(rename = "include-anachronisms")]
    IncludeAnachronisms,
}

fn job_view(id: &str, job: &Job, ttl: Duration) -> Value {
    let remaining = ttl.saturating_sub(job.created.elapsed()).as_secs();
    json!({
        "id": id,
        "example": job.example_id,
        "date": job.date,
        "policy": job.policy,
        "mode": job.result.certificate.mode,
        "expires_in_secs": remaining,
        "zip_sha256": job.result.zip_sha256,
        "zip_bytes": job.result.zip.len(),
        "certificate": job.result.certificate,
        "urls": {
            "self": format!("/api/resurrections/{id}"),
            "zip": format!("/api/resurrections/{id}/archive.zip"),
            "preview": format!("/p/{id}/"),
        }
    })
}

async fn create(
    State(app): State<Arc<App>>,
    Extension(client): Extension<ClientKey>,
    body: Result<Bytes, BytesRejection>,
) -> Result<(StatusCode, Json<Value>), ApiError> {
    if let Some(wait) = app.rate_check(&client.0) {
        let mut e = ApiError::new(
            StatusCode::TOO_MANY_REQUESTS,
            "rate_limited",
            "Too many requests. Wait a moment and try again.",
        );
        e.retry_after = Some(wait);
        return Err(e);
    }
    let bytes = body.map_err(|r| {
        if r.status() == StatusCode::PAYLOAD_TOO_LARGE {
            ApiError::new(
                StatusCode::PAYLOAD_TOO_LARGE,
                "body_too_large",
                format!("The request body is limited to {MAX_BODY_BYTES} bytes."),
            )
        } else {
            ApiError::bad("bad_body", "Could not read the request body.")
        }
    })?;
    let req: CreateBody = serde_json::from_slice(&bytes)
        .map_err(|e| ApiError::bad("bad_request", format!("Invalid request: {e}")))?;

    let example = app
        .example(&req.example)
        .ok_or_else(|| ApiError::bad("unknown_example", "No such example."))?;
    let target = parse_day_end(&req.date)
        .ok_or_else(|| ApiError::bad("bad_date", "Use a real date written YYYY-MM-DD."))?;

    let job_req = PipelineRequest {
        site: example.info.site.clone(),
        page: example.info.page.clone(),
        target,
        include_anachronisms: matches!(req.policy, PolicyField::IncludeAnachronisms),
    };
    let source = example.source.clone();
    let outcome = tokio::task::spawn_blocking(move || resurrect(&*source, &job_req))
        .await
        .map_err(|_| {
            ApiError::new(
                StatusCode::INTERNAL_SERVER_ERROR,
                "internal",
                "The job failed.",
            )
        })?;

    let result = match outcome {
        Ok(r) => r,
        Err(PipelineError::NoCapture { verdict, detail }) => {
            let mut e = ApiError::new(
                StatusCode::UNPROCESSABLE_ENTITY,
                "no_capture",
                "The archive has nothing for this page at that date.",
            );
            e.extra = Some(json!({
                "verdict": verdict.as_str(),
                "detail": detail,
                "next_step": "Try a later date, or allow files captured after the date.",
            }));
            return Err(e);
        }
        Err(other) => {
            tracing::warn!(error = ?other, "pipeline error");
            return Err(ApiError::new(
                StatusCode::UNPROCESSABLE_ENTITY,
                "cannot_restore",
                format!("{other:?}"),
            ));
        }
    };

    let id = new_id()?;
    let job = Job {
        example_id: example.info.id.clone(),
        date: req.date.clone(),
        policy: match req.policy {
            PolicyField::Strict => "strict",
            PolicyField::IncludeAnachronisms => "with-anachronisms",
        },
        created: Instant::now(),
        result,
    };
    let view = job_view(&id, &job, app.cfg.job_ttl);
    app.insert(id, job);
    Ok((StatusCode::CREATED, Json(view)))
}

async fn show(
    State(app): State<Arc<App>>,
    Path(id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let job = app.job(&id).ok_or_else(ApiError::not_found)?;
    Ok(Json(job_view(&id, &job, app.cfg.job_ttl)))
}

async fn download(
    State(app): State<Arc<App>>,
    Path(id): Path<String>,
) -> Result<Response, ApiError> {
    let job = app.job(&id).ok_or_else(ApiError::not_found)?;
    let name = format!(
        "revenant-{}-{}-{}.zip",
        job.example_id, job.date, job.policy
    );
    let mut res = Response::new(Body::from(job.result.zip.clone()));
    let h = res.headers_mut();
    h.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/zip"),
    );
    if let Ok(v) = HeaderValue::from_str(&format!("attachment; filename=\"{name}\"")) {
        h.insert(header::CONTENT_DISPOSITION, v);
    }
    Ok(res)
}

async fn preview_redirect(Path(id): Path<String>) -> Response {
    if !is_job_id(&id) {
        return ApiError::not_found().into_response();
    }
    Redirect::permanent(&format!("/p/{id}/")).into_response()
}

async fn preview_page(
    State(app): State<Arc<App>>,
    Path(id): Path<String>,
) -> Result<Response, ApiError> {
    let job = app.job(&id).ok_or_else(ApiError::not_found)?;
    let page = job.result.certificate.page.clone();
    serve_entry(&app, &job, &page)
}

async fn preview_file(
    State(app): State<Arc<App>>,
    Path((id, path)): Path<(String, String)>,
) -> Result<Response, ApiError> {
    let job = app.job(&id).ok_or_else(ApiError::not_found)?;
    serve_entry(&app, &job, &path)
}

/// Serve one file, and only a file that is in this job's ZIP (A3.1).
fn serve_entry(app: &App, job: &Job, path: &str) -> Result<Response, ApiError> {
    let entry = job
        .result
        .entries
        .iter()
        .find(|e| e.path == path)
        .ok_or_else(ApiError::not_found)?;
    let mut res = Response::new(Body::from(entry.bytes.clone()));
    let h = res.headers_mut();
    h.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static(content_type(path)),
    );
    let csp = format!(
        "sandbox; default-src 'none'; img-src 'self' data:; style-src 'self' 'unsafe-inline'; \
         font-src 'self'; frame-ancestors {}",
        app.cfg.frame_ancestors
    );
    if let Ok(v) = HeaderValue::from_str(&csp) {
        h.insert(header::CONTENT_SECURITY_POLICY, v);
    }
    h.insert(
        header::HeaderName::from_static("x-robots-tag"),
        HeaderValue::from_static("noindex, nofollow"),
    );
    h.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    Ok(res)
}

// ---------------------------------------------------------------- helpers

/// Content types are an allowlist. Everything else is a download (A3.3).
fn content_type(path: &str) -> &'static str {
    let ext = path.rsplit('.').next().unwrap_or("").to_ascii_lowercase();
    match ext.as_str() {
        "html" | "htm" => "text/html",
        "css" => "text/css",
        "gif" => "image/gif",
        "jpg" | "jpeg" => "image/jpeg",
        "png" => "image/png",
        "json" => "application/json",
        "txt" => "text/plain",
        _ => "application/octet-stream",
    }
}

/// `YYYY-MM-DD` to Unix seconds at 23:59:59 UTC that day.
fn parse_day_end(date: &str) -> Option<i64> {
    let b = date.as_bytes();
    let shaped = b.len() == 10
        && b[4] == b'-'
        && b[7] == b'-'
        && b.iter()
            .enumerate()
            .all(|(i, c)| i == 4 || i == 7 || c.is_ascii_digit());
    if !shaped {
        return None;
    }
    let stamp = format!("{}{}{}235959", &date[0..4], &date[5..7], &date[8..10]);
    parse_cdx_timestamp(&stamp)
}

fn is_job_id(id: &str) -> bool {
    id.len() == 32
        && id
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

fn new_id() -> Result<String, ApiError> {
    let mut raw = [0u8; 16];
    getrandom::fill(&mut raw).map_err(|_| {
        ApiError::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            "internal",
            "No randomness available.",
        )
    })?;
    Ok(raw.iter().map(|b| format!("{b:02x}")).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn day_end_is_the_last_second_of_the_day() {
        let t = parse_day_end("2000-03-02").unwrap();
        assert_eq!(t, parse_cdx_timestamp("20000302235959").unwrap());
    }

    #[test]
    fn bad_dates_are_rejected() {
        for bad in [
            "",
            "2000-3-2",
            "2000/03/02",
            "2000-13-01",
            "2000-02-30",
            "20000302",
            "x000-03-02",
        ] {
            assert_eq!(parse_day_end(bad), None, "{bad}");
        }
    }

    #[test]
    fn content_types_are_an_allowlist() {
        assert_eq!(content_type("a.HTML"), "text/html");
        assert_eq!(content_type("a.svg"), "application/octet-stream");
        assert_eq!(content_type("a.exe"), "application/octet-stream");
        assert_eq!(content_type("noext"), "application/octet-stream");
    }

    #[test]
    fn job_ids_are_32_lowercase_hex() {
        assert!(is_job_id(&"a".repeat(32)));
        assert!(!is_job_id(&"A".repeat(32)));
        assert!(!is_job_id("short"));
    }
}
