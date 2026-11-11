//! Revenant HTTP API. See `.kiro/specs/resurrection-api/requirements.md`.
//!
//! JSON lives under `/api`. Restored files are served only under `/p`, with a sandboxing CSP.

use std::collections::{HashMap, VecDeque};
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use axum::body::{Body, Bytes};
use axum::extract::rejection::BytesRejection;
use axum::extract::{ConnectInfo, DefaultBodyLimit, Extension, Path, Query, Request, State};
use axum::http::{header, HeaderValue, StatusCode};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Redirect, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use revenant_core::annotate::{annotate_html, Annotation};
use revenant_core::pipeline::{
    resurrect, resurrect_with, Limits, PipelineError, Request as PipelineRequest, Resurrection,
    SeededSource,
};
use revenant_core::time::parse_cdx_timestamp;
use serde::{Deserialize, Serialize};

pub mod live;
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
    /// How many trusted proxies sit in front, counted from the right of `X-Forwarded-For` (A1.6).
    pub proxy_hops: usize,
    /// Upper bound on the bytes of all retained jobs (A1.7).
    pub max_job_bytes: usize,
    pub frame_ancestors: String,
    /// The one archive host live mode may contact (L1.2).
    pub archive_base: String,
    /// Minimum gap between archive requests (L3.1).
    pub archive_gap: Duration,
    pub live_max_concurrent: usize,
    pub live_rate_limit: u32,
    pub live_rate_window: Duration,
    /// Timeline lookups per client per window (T1.3).
    pub timeline_rate_limit: u32,
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
            proxy_hops: 1,
            max_job_bytes: 64 * 1024 * 1024,
            frame_ancestors: "'self'".to_string(),
            archive_base: "https://web.archive.org".to_string(),
            archive_gap: Duration::from_millis(1200),
            live_max_concurrent: 2,
            live_rate_limit: 6,
            live_rate_window: Duration::from_secs(600),
            timeline_rate_limit: 20,
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

impl Job {
    /// Bytes this job keeps in memory: the ZIP plus the files used for previews.
    fn weight(&self) -> usize {
        self.result.zip.len()
            + self
                .result
                .entries
                .iter()
                .map(|e| e.bytes.len())
                .sum::<usize>()
    }
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
    /// Sum of `Job::weight()` over `map`.
    bytes: usize,
}

impl Store {
    fn remove(&mut self, id: &str) {
        if let Some(job) = self.map.remove(id) {
            self.bytes = self.bytes.saturating_sub(job.weight());
        }
        self.order.retain(|o| o != id);
    }
}

enum RunState {
    Running,
    Done {
        strict: String,
        nearest: String,
        dates_only: String,
    },
    Failed {
        code: &'static str,
        message: String,
    },
}

struct Run {
    created: Instant,
    progress: Arc<Mutex<live::Progress>>,
    state: Mutex<RunState>,
    notes: Vec<String>,
}

#[derive(Default)]
struct RunStore {
    map: HashMap<String, Arc<Run>>,
    order: VecDeque<String>,
}

pub struct App {
    cfg: Config,
    examples: Vec<Example>,
    jobs: Mutex<Store>,
    hits: Mutex<HashMap<String, (Instant, u32)>>,
    live_hits: Mutex<HashMap<String, (Instant, u32)>>,
    timeline_hits: Mutex<HashMap<String, (Instant, u32)>>,
    runs: Mutex<RunStore>,
    live_active: AtomicUsize,
    archive: Arc<live::ArchiveClient>,
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
        // The preview CSP is built from this value on every request, so a bad value would
        // silently remove the sandbox. Refuse to start instead (S1).
        preview_csp(&cfg.frame_ancestors)
            .ok_or_else(|| "REVENANT_FRAME_ANCESTORS is not a valid header value".to_string())?;
        let archive = live::ArchiveClient::new(&cfg.archive_base, cfg.archive_gap)?;
        Ok(Arc::new(Self {
            cfg,
            examples,
            jobs: Mutex::new(Store::default()),
            hits: Mutex::new(HashMap::new()),
            live_hits: Mutex::new(HashMap::new()),
            timeline_hits: Mutex::new(HashMap::new()),
            runs: Mutex::new(RunStore::default()),
            live_active: AtomicUsize::new(0),
            archive: Arc::new(archive),
        }))
    }

    fn example(&self, id: &str) -> Option<&Example> {
        self.examples.iter().find(|e| e.info.id == id)
    }

    fn insert(&self, id: String, job: Job) {
        let mut store = self.jobs.lock().expect("job store lock");
        let ttl = self.cfg.job_ttl;
        // Drop expired jobs, then make room by count and by bytes (A1.4, A1.7).
        let expired: Vec<String> = store
            .map
            .iter()
            .filter(|(_, j)| j.created.elapsed() >= ttl)
            .map(|(k, _)| k.clone())
            .collect();
        for k in expired {
            store.remove(&k);
        }
        let weight = job.weight();
        while store.map.len() >= self.cfg.max_jobs
            || (store.bytes + weight > self.cfg.max_job_bytes && !store.map.is_empty())
        {
            match store.order.front().cloned() {
                Some(old) => store.remove(&old),
                None => break,
            }
        }
        store.bytes += weight;
        store.order.push_back(id.clone());
        store.map.insert(id, Arc::new(job));
    }

    fn job(&self, id: &str) -> Option<Arc<Job>> {
        let mut store = self.jobs.lock().expect("job store lock");
        let job = store.map.get(id)?.clone();
        if job.created.elapsed() >= self.cfg.job_ttl {
            store.remove(id);
            return None;
        }
        Some(job)
    }

    /// Returns the seconds to wait when the client is over its limit.
    fn rate_check(&self, key: &str) -> Option<u64> {
        Self::window_check(&self.hits, key, self.cfg.rate_limit, self.cfg.rate_window)
    }

    fn live_rate_check(&self, key: &str) -> Option<u64> {
        Self::window_check(
            &self.live_hits,
            key,
            self.cfg.live_rate_limit,
            self.cfg.live_rate_window,
        )
    }

    fn timeline_rate_check(&self, key: &str) -> Option<u64> {
        Self::window_check(
            &self.timeline_hits,
            key,
            self.cfg.timeline_rate_limit,
            self.cfg.live_rate_window,
        )
    }

    fn window_check(
        table: &Mutex<HashMap<String, (Instant, u32)>>,
        key: &str,
        limit: u32,
        window: Duration,
    ) -> Option<u64> {
        let mut hits = table.lock().expect("rate lock");
        let now = Instant::now();
        if hits.len() > 10_000 {
            hits.retain(|_, (start, _)| now.duration_since(*start) < window);
        }
        let entry = hits.entry(key.to_string()).or_insert((now, 0));
        if now.duration_since(entry.0) >= window {
            *entry = (now, 0);
        }
        entry.1 += 1;
        if entry.1 > limit {
            let wait = window.saturating_sub(now.duration_since(entry.0));
            return Some(wait.as_secs().max(1));
        }
        None
    }

    fn insert_run(&self, id: String, run: Run) -> Arc<Run> {
        let run = Arc::new(run);
        let mut store = self.runs.lock().expect("run store lock");
        let ttl = self.cfg.job_ttl;
        let expired: Vec<String> = store
            .map
            .iter()
            .filter(|(_, r)| r.created.elapsed() >= ttl)
            .map(|(k, _)| k.clone())
            .collect();
        for k in expired {
            store.map.remove(&k);
            store.order.retain(|o| o != &k);
        }
        while store.map.len() >= 200 {
            match store.order.pop_front() {
                Some(old) => {
                    store.map.remove(&old);
                }
                None => break,
            }
        }
        store.order.push_back(id.clone());
        store.map.insert(id, run.clone());
        run
    }

    fn run(&self, id: &str) -> Option<Arc<Run>> {
        let store = self.runs.lock().expect("run store lock");
        let run = store.map.get(id)?.clone();
        (run.created.elapsed() < self.cfg.job_ttl).then_some(run)
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
        .route(
            "/api/live",
            post(live_create).layer(DefaultBodyLimit::max(MAX_BODY_BYTES)),
        )
        .route("/api/live/{id}", get(live_show))
        .route(
            "/api/timeline",
            post(timeline).layer(DefaultBodyLimit::max(MAX_BODY_BYTES)),
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
        let values: Vec<&str> = req
            .headers()
            .get_all("x-forwarded-for")
            .iter()
            .filter_map(|v| v.to_str().ok())
            .collect();
        forwarded_client(&values, app.cfg.proxy_hops)
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

/// The client address from `X-Forwarded-For`, trusting `hops` proxies from the right (A1.6).
///
/// Each proxy appends the address it saw, so the right-most entries are written by proxies we
/// trust and the left-most are written by the client. `hops = 1` takes the last entry.
pub fn forwarded_client(header_values: &[&str], hops: usize) -> Option<String> {
    if hops == 0 {
        return None;
    }
    let entries: Vec<&str> = header_values
        .iter()
        .flat_map(|v| v.split(','))
        .map(str::trim)
        .filter(|e| !e.is_empty())
        .collect();
    let entry = entries.get(entries.len().checked_sub(hops)?)?;
    entry
        .parse::<std::net::IpAddr>()
        .ok()
        .map(|ip| ip.to_string())
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
    #[serde(rename = "nearest")]
    Nearest,
    #[serde(rename = "dates-only")]
    DatesOnly,
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
        nearest: matches!(req.policy, PolicyField::Nearest),
        dates_only: matches!(req.policy, PolicyField::DatesOnly),
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
                "This page could not be restored.",
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
            PolicyField::Nearest => "nearest",
            PolicyField::DatesOnly => "dates-only",
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

#[derive(Debug, Deserialize, Default)]
struct PreviewQuery {
    #[serde(default)]
    annotate: Option<String>,
}

async fn preview_page(
    State(app): State<Arc<App>>,
    Path(id): Path<String>,
    Query(q): Query<PreviewQuery>,
) -> Result<Response, ApiError> {
    let job = app.job(&id).ok_or_else(ApiError::not_found)?;
    let page = job.result.certificate.page.clone();
    // Only the page can be annotated, and only when asked (E3.1, E3.2).
    if q.annotate.as_deref() == Some("1") {
        return serve_annotated(&app, &job, &page);
    }
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
    preview_response(app, entry.bytes.clone(), content_type(path))
}

/// The page with verdicts drawn on it (E3.1). Built from the certificate, so it cannot disagree
/// with the ledger, and never stored.
fn serve_annotated(app: &App, job: &Job, page: &str) -> Result<Response, ApiError> {
    let entry = job
        .result
        .entries
        .iter()
        .find(|e| e.path == page)
        .ok_or_else(ApiError::not_found)?;
    let annotation = Annotation::from_certificate(&job.result.certificate);
    let bytes = annotate_html(&entry.bytes, &annotation).map_err(|e| {
        tracing::warn!(error = ?e, "annotation failed");
        ApiError::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            "internal",
            "The annotated view could not be built.",
        )
    })?;
    preview_response(app, bytes, content_type(page))
}

/// Every preview response carries the same headers, annotated or not (A3.2, E3.3).
fn preview_response(
    app: &App,
    bytes: Vec<u8>,
    content_type: &'static str,
) -> Result<Response, ApiError> {
    let mut res = Response::new(Body::from(bytes));
    let h = res.headers_mut();
    h.insert(header::CONTENT_TYPE, HeaderValue::from_static(content_type));
    // Fail closed: if the sandboxing policy cannot be built, serve nothing (S1).
    let csp = preview_csp(&app.cfg.frame_ancestors).ok_or_else(|| {
        tracing::error!("preview CSP could not be built");
        ApiError::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            "internal",
            "The preview could not be served safely.",
        )
    })?;
    h.insert(header::CONTENT_SECURITY_POLICY, csp);
    h.insert(
        header::HeaderName::from_static("x-robots-tag"),
        HeaderValue::from_static("noindex, nofollow"),
    );
    h.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    Ok(res)
}

// ---------------------------------------------------------------- live mode

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct LiveBody {
    url: String,
    date: String,
    #[serde(default)]
    rights: bool,
}

/// Counts a running live job and releases the slot when dropped.
struct LiveSlot(Arc<App>);

impl Drop for LiveSlot {
    fn drop(&mut self) {
        self.0.live_active.fetch_sub(1, Ordering::SeqCst);
    }
}

async fn live_create(
    State(app): State<Arc<App>>,
    Extension(client): Extension<ClientKey>,
    body: Result<Bytes, BytesRejection>,
) -> Result<(StatusCode, Json<Value>), ApiError> {
    if let Some(wait) = app.live_rate_check(&client.0) {
        let mut e = ApiError::new(
            StatusCode::TOO_MANY_REQUESTS,
            "rate_limited",
            "Live restores are limited so the archive stays polite. Try again shortly.",
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
    let req: LiveBody = serde_json::from_slice(&bytes)
        .map_err(|e| ApiError::bad("bad_request", format!("Invalid request: {e}")))?;

    if !req.rights {
        return Err(ApiError::bad(
            "rights_required",
            "Confirm that the site is yours, or that you have the right to restore it.",
        ));
    }
    let target = live::parse_target(&req.url).map_err(|e| ApiError::bad("bad_url", e.0))?;
    let target_ts = parse_day_end(&req.date)
        .ok_or_else(|| ApiError::bad("bad_date", "Use a real date written YYYY-MM-DD."))?;

    // Take a slot, or say the service is busy (L3.3).
    let active = app.live_active.fetch_add(1, Ordering::SeqCst);
    if active >= app.cfg.live_max_concurrent {
        app.live_active.fetch_sub(1, Ordering::SeqCst);
        let mut e = ApiError::new(
            StatusCode::SERVICE_UNAVAILABLE,
            "busy",
            "Other live restores are running. Try again in a minute.",
        );
        e.retry_after = Some(30);
        return Err(e);
    }
    let slot = LiveSlot(app.clone());

    let run_id = new_id()?;
    let progress = Arc::new(Mutex::new(live::Progress {
        stage: "Starting".to_string(),
        requests: 0,
    }));
    let mut notes = Vec::new();
    if target.dropped_query {
        notes.push(
            "The query string and fragment were ignored. Only the site and path are used."
                .to_string(),
        );
    }
    let run = app.insert_run(
        run_id.clone(),
        Run {
            created: Instant::now(),
            progress: progress.clone(),
            state: Mutex::new(RunState::Running),
            notes,
        },
    );

    let worker_app = app.clone();
    let date = req.date.clone();
    tokio::task::spawn_blocking(move || {
        let _slot = slot;
        let outcome = run_live(&worker_app, &target, target_ts, &date, progress);
        let mut state = run.state.lock().expect("run state lock");
        *state = match outcome {
            Ok((strict, nearest, dates_only)) => RunState::Done {
                strict,
                nearest,
                dates_only,
            },
            Err((code, message)) => RunState::Failed { code, message },
        };
    });

    Ok((
        StatusCode::ACCEPTED,
        Json(
            json!({ "id": run_id, "status": "running", "urls": { "self": format!("/api/live/{run_id}") } }),
        ),
    ))
}

/// One set of archive requests feeds both the strict and the nearest-capture restore (L3.4).
fn run_live(
    app: &Arc<App>,
    target: &live::LiveTarget,
    target_ts: i64,
    date: &str,
    progress: Arc<Mutex<live::Progress>>,
) -> Result<(String, String, String), (&'static str, String)> {
    let source = live::LiveSource::new(app.archive.clone(), target_ts, progress.clone());
    let limits = Limits::new(live::MAX_LIVE_ASSETS, live::MAX_LIVE_TOTAL_BYTES);
    let make = |nearest: bool, dates_only: bool| PipelineRequest {
        site: target.site.clone(),
        page: target.page.clone(),
        target: target_ts,
        include_anachronisms: false,
        nearest,
        dates_only,
    };

    let strict =
        resurrect_with(&source, &make(false, false), limits).map_err(map_pipeline_error)?;
    let nearest =
        resurrect_with(&source, &make(true, false), limits).map_err(map_pipeline_error)?;
    // A comparison made from capture dates alone. It reuses what the strict run fetched (A5.2).
    let dates_only =
        resurrect_with(&source, &make(false, true), limits).map_err(map_pipeline_error)?;

    let save =
        |result: Resurrection, policy: &'static str| -> Result<String, (&'static str, String)> {
            let id = new_id().map_err(|_| ("internal", "No randomness available.".to_string()))?;
            app.insert(
                id.clone(),
                Job {
                    example_id: format!("live-{}", target.site),
                    date: date.to_string(),
                    policy,
                    created: Instant::now(),
                    result,
                },
            );
            Ok(id)
        };
    let strict_id = save(strict, "strict")?;
    let nearest_id = save(nearest, "nearest")?;
    let dates_only_id = save(dates_only, "dates-only")?;
    Ok((strict_id, nearest_id, dates_only_id))
}

fn map_pipeline_error(e: PipelineError) -> (&'static str, String) {
    match e {
        PipelineError::NoCapture { detail, .. } => (
            "no_capture",
            format!("The archive has nothing usable for this page at that date. {detail}"),
        ),
        // A failed lookup is not "no capture" (L2.2).
        PipelineError::PageUnknown(why) => {
            tracing::warn!(%why, "archive did not answer for the page");
            (
                "archive_unavailable",
                "The archive could not be reached for this page. This is not the same as nothing being there.".to_string(),
            )
        }
        PipelineError::Source(why) => {
            tracing::warn!(%why, "archive could not supply the page");
            (
                "archive_unavailable",
                "The archive could not supply this page. This is not the same as nothing being there.".to_string(),
            )
        }
        PipelineError::TooManyAssets { found, cap } => (
            "too_many_assets",
            format!("This page uses {found} files. Live restores are limited to {cap}."),
        ),
        PipelineError::TooLarge { cap_bytes } => (
            "too_large",
            format!(
                "This page is larger than the live limit of {} MiB.",
                cap_bytes / (1024 * 1024)
            ),
        ),
        other => {
            tracing::warn!(error = ?other, "live pipeline error");
            (
                "cannot_restore",
                "This page could not be restored.".to_string(),
            )
        }
    }
}

async fn live_show(
    State(app): State<Arc<App>>,
    Path(id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let run = app.run(&id).ok_or_else(ApiError::not_found)?;
    let state = run.state.lock().expect("run state lock");
    let body = match &*state {
        RunState::Running => {
            let p = run.progress.lock().expect("progress lock");
            json!({ "id": id, "status": "running", "stage": p.stage, "requests": p.requests, "notes": run.notes })
        }
        RunState::Failed { code, message } => json!({
            "id": id,
            "status": "failed",
            "error": { "code": code, "message": message },
            "notes": run.notes,
        }),
        RunState::Done {
            strict,
            nearest,
            dates_only,
        } => {
            let s = app.job(strict).ok_or_else(ApiError::not_found)?;
            let n = app.job(nearest).ok_or_else(ApiError::not_found)?;
            let d = app.job(dates_only).ok_or_else(ApiError::not_found)?;
            json!({
                "id": id,
                "status": "done",
                "notes": run.notes,
                "strict": job_view(strict, &s, app.cfg.job_ttl),
                "nearest": job_view(nearest, &n, app.cfg.job_ttl),
                "dates_only": job_view(dates_only, &d, app.cfg.job_ttl),
            })
        }
    };
    Ok(Json(body))
}

// ---------------------------------------------------------------- timeline (T)

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct TimelineBody {
    url: String,
}

/// One archive question, run on a blocking thread because the archive client is synchronous.
async fn timeline(
    State(app): State<Arc<App>>,
    Extension(client): Extension<ClientKey>,
    body: Result<Bytes, BytesRejection>,
) -> Result<Json<Value>, ApiError> {
    if let Some(wait) = app.timeline_rate_check(&client.0) {
        let mut e = ApiError::new(
            StatusCode::TOO_MANY_REQUESTS,
            "rate_limited",
            "Too many lookups. Wait a moment and try again.",
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
    let req: TimelineBody = serde_json::from_slice(&bytes)
        .map_err(|e| ApiError::bad("bad_request", format!("Invalid request: {e}")))?;
    let target = live::parse_target(&req.url).map_err(|e| ApiError::bad("bad_url", e.0))?;
    let label = target.label();

    let client = app.archive.clone();
    let key = label.clone();
    let rows = tokio::task::spawn_blocking(move || client.timeline(&key))
        .await
        .map_err(|_| {
            ApiError::new(
                StatusCode::INTERNAL_SERVER_ERROR,
                "internal",
                "The lookup failed.",
            )
        })?
        .map_err(|e| {
            tracing::warn!(why = %e.0, "timeline lookup failed");
            ApiError::new(
                StatusCode::BAD_GATEWAY,
                "archive_unavailable",
                "The archive could not be reached. This is not the same as nothing being there.",
            )
        })?;

    let truncated = rows.len() > live::MAX_TIMELINE_MONTHS;
    let mut months: Vec<Value> = Vec::new();
    for row in rows.iter().take(live::MAX_TIMELINE_MONTHS) {
        // Rows with a timestamp we cannot read are dropped, never guessed at.
        if parse_cdx_timestamp(&row.timestamp).is_some() {
            months.push(json!({
                "month": &row.timestamp[..6],
                "first_capture": row.timestamp,
                "digest": row.digest,
            }));
        }
    }
    let first = months
        .first()
        .and_then(|v| v["first_capture"].as_str().map(str::to_string));
    let latest = months
        .last()
        .and_then(|v| v["month"].as_str().map(str::to_string));
    Ok(Json(json!({
        "site": target.site,
        "page": target.page,
        "label": label,
        "first_capture": first,
        // True of the whole history: the last month that holds a capture (T1.7).
        "latest_month": latest,
        "month_count": months.len(),
        "months": months,
        "truncated": truncated,
        "notes": if target.dropped_query {
            json!(["The query string and fragment were ignored. Only the site and path are used."])
        } else {
            json!([])
        },
    })))
}

// ---------------------------------------------------------------- helpers

/// The sandboxing policy for previews. `None` when `frame_ancestors` cannot form a valid header.
fn preview_csp(frame_ancestors: &str) -> Option<HeaderValue> {
    if frame_ancestors.chars().any(|c| c.is_control() || c == ';') {
        return None;
    }
    HeaderValue::from_str(&format!(
        "sandbox; default-src 'none'; img-src 'self' data:; style-src 'self' 'unsafe-inline'; \
         font-src 'self'; frame-ancestors {frame_ancestors}"
    ))
    .ok()
}

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
    fn the_client_address_comes_from_the_right_not_the_left() {
        // A client sends "1.2.3.4"; the one trusted proxy appends the address it saw.
        let h = ["1.2.3.4, 203.0.113.9"];
        assert_eq!(forwarded_client(&h, 1).as_deref(), Some("203.0.113.9"));
        // Two trusted proxies: the second-from-right is the client as the outer proxy saw it.
        assert_eq!(
            forwarded_client(&["1.2.3.4, 203.0.113.9, 10.0.0.1"], 2).as_deref(),
            Some("203.0.113.9")
        );
        // Separate header lines are joined in order.
        assert_eq!(
            forwarded_client(&["1.2.3.4", "203.0.113.9"], 1).as_deref(),
            Some("203.0.113.9")
        );
    }

    #[test]
    fn unusable_forwarded_headers_are_ignored() {
        assert_eq!(forwarded_client(&[], 1), None);
        assert_eq!(forwarded_client(&["203.0.113.9"], 0), None);
        assert_eq!(
            forwarded_client(&["203.0.113.9"], 2),
            None,
            "too few entries"
        );
        assert_eq!(forwarded_client(&["1.2.3.4, not-an-ip"], 1), None);
        assert_eq!(forwarded_client(&["1.2.3.4, <script>"], 1), None);
        assert_eq!(
            forwarded_client(&["1.2.3.4, "], 1).as_deref(),
            Some("1.2.3.4")
        );
        assert_eq!(
            forwarded_client(&["2001:db8::1"], 1).as_deref(),
            Some("2001:db8::1")
        );
    }

    #[test]
    fn job_ids_are_32_lowercase_hex() {
        assert!(is_job_id(&"a".repeat(32)));
        assert!(!is_job_id(&"A".repeat(32)));
        assert!(!is_job_id("short"));
    }
}
