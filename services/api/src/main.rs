use std::net::SocketAddr;
use std::path::PathBuf;

use revenant_api::{router, App, Config};

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "revenant_api=info".into()),
        )
        .init();

    let mut cfg = Config::default();
    if let Ok(dir) = std::env::var("REVENANT_FIXTURES") {
        cfg.fixtures_dir = PathBuf::from(dir);
    }
    cfg.web_dir = std::env::var("REVENANT_WEB_DIR").ok().map(PathBuf::from);
    cfg.trust_proxy = std::env::var("REVENANT_TRUST_PROXY").as_deref() == Ok("1");
    if let Some(hops) = std::env::var("REVENANT_PROXY_HOPS")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
    {
        cfg.proxy_hops = hops;
    }
    if let Ok(v) = std::env::var("REVENANT_FRAME_ANCESTORS") {
        cfg.frame_ancestors = v;
    }
    let port: u16 = std::env::var("PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(8080);

    let app = match App::new(cfg) {
        Ok(app) => app,
        Err(e) => {
            eprintln!("cannot start: {e}");
            std::process::exit(1);
        }
    };
    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    let listener = match tokio::net::TcpListener::bind(addr).await {
        Ok(l) => l,
        Err(e) => {
            eprintln!("cannot bind {addr}: {e}");
            std::process::exit(1);
        }
    };
    tracing::info!(%addr, "revenant api listening");
    let server = axum::serve(
        listener,
        router(app).into_make_service_with_connect_info::<SocketAddr>(),
    )
    .with_graceful_shutdown(async {
        let _ = tokio::signal::ctrl_c().await;
    });
    if let Err(e) = server.await {
        eprintln!("server error: {e}");
        std::process::exit(1);
    }
}
