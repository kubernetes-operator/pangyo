//! MemoNest API 서버 실행 파일.
use std::env;

use memonest_server::{router, BIND_ENV, DEFAULT_BIND_ADDRESS, LOG_ENV, VERSION};
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> std::io::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_env(LOG_ENV)
                .unwrap_or_else(|_| EnvFilter::new("info,tower_http=info")),
        )
        .init();

    let addr = env::var(BIND_ENV).unwrap_or_else(|_| DEFAULT_BIND_ADDRESS.to_owned());
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    tracing::info!(
        version = VERSION,
        addr = %listener.local_addr()?,
        "MemoNest API listening (no sync/auth endpoints implemented)"
    );
    axum::serve(listener, router())
        .with_graceful_shutdown(shutdown_signal())
        .await
}

/// Ctrl+C 또는 SIGTERM(Kubernetes Pod 종료)을 받으면 진행 중 요청을 마치고 종료합니다.
async fn shutdown_signal() {
    let ctrl_c = async {
        let _ = tokio::signal::ctrl_c().await;
    };
    #[cfg(unix)]
    let terminate = async {
        match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            Ok(mut s) => {
                s.recv().await;
            }
            Err(_) => std::future::pending::<()>().await,
        }
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();
    tokio::select! {
        () = ctrl_c => {},
        () = terminate => {},
    }
    tracing::info!("shutdown signal received");
}
