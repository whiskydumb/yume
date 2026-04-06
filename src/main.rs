mod state;
mod features;
mod error;

use axum::Router;
use axum::http::{header, HeaderValue};
use socket2::{Domain, Protocol, Socket, Type};
use sqlx::postgres::PgPoolOptions;
use state::AppState;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::TcpListener;
use tokio_util::sync::CancellationToken;
use tower::ServiceBuilder;
use tower_http::services::ServeDir;
use tower_http::set_header::SetResponseHeaderLayer;
use tracing_subscriber::EnvFilter;

#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

fn bind(addr: SocketAddr, backlog: i32) -> std::io::Result<TcpListener> {
    let domain = match addr {
        SocketAddr::V4(_) => Domain::IPV4,
        SocketAddr::V6(_) => Domain::IPV6,
    };
    let socket = Socket::new(domain, Type::STREAM, Some(Protocol::TCP))?;
    socket.set_reuse_address(true)?;
    #[cfg(unix)]
    let _ = socket.set_reuse_port(true);
    socket.set_nonblocking(true)?;
    socket.bind(&addr.into())?;
    socket.listen(backlog)?;

    TcpListener::from_std(socket.into())
}

#[tokio::main(flavor = "multi_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    dotenvy::dotenv().ok();

    let (non_blocking, _guard) = tracing_appender::non_blocking(std::io::stdout());
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"))
        )
        .compact()
        .with_writer(non_blocking)
        .init();

    let addr: SocketAddr = std::env::var("ADDR")
        .unwrap_or_else(|_| "0.0.0.0:3000".to_string())
        .parse()?;
    let listener = bind(addr, 8192)?;

    let threads = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(1);

    println!("🌸 yume");
    println!("   addr     →  http://{addr}");
    println!("   threads  →  {threads}");

    let database_url = std::env::var("DATABASE_URL").expect("DATABASE_URL must be set");
    let db = PgPoolOptions::new()
        .max_connections(20)
        .connect(&database_url)
        .await?;

    // tracing::info!("db connected");
    println!("   db       →  connected");

    let site_cache = features::sites::cache::new();
    features::sites::cache::reload(&site_cache, &db).await?;

    let jwt_secret: Arc<[u8]> = std::env::var("JWT_SECRET")
        .expect("JWT_SECRET must be set")
        .into_bytes()
        .into();

    let admin_password_hash: Arc<str> = std::env::var("ADMIN_PASSWORD_HASH")
        .expect("ADMIN_PASSWORD_HASH must be set")
        .into();

    let jwt_expiry_hours: u64 = std::env::var("JWT_EXPIRY_HOURS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(24);

    let base_url = state::BaseUrl(
        std::env::var("BASE_URL")
            .unwrap_or_else(|_| "https://yume.example".to_string())
            .trim_end_matches('/')
            .into(),
    );

    let scan_notify = Arc::new(tokio::sync::Notify::new());
    let shutdown = CancellationToken::new();

    let state = AppState { db: db.clone(), site_cache: site_cache.clone(), jwt_secret, admin_password_hash, jwt_expiry_hours, base_url, scan_notify: scan_notify.clone() };

    tokio::spawn(features::checker::worker::run(db.clone(), site_cache, scan_notify, shutdown.clone()));

    let app = Router::new()
        .merge(features::pages::router())
        .merge(features::auth::router(state.clone()))
        .merge(features::sites::router())
        .merge(features::applications::router())
        .nest_service("/static",
            ServiceBuilder::new()
                .layer(SetResponseHeaderLayer::overriding(
                    header::CACHE_CONTROL,
                    HeaderValue::from_static("public, max-age=86400"),
                ))
                .service(ServeDir::new("static"))
        )
        .nest_service("/data",
            ServiceBuilder::new()
                .layer(SetResponseHeaderLayer::overriding(
                    header::CACHE_CONTROL,
                    HeaderValue::from_static("public, max-age=604800"),
                ))
                .service(ServeDir::new("data"))
        )
        .with_state(state);

    let shutdown_token = shutdown.clone();
    axum::serve(listener, app.into_make_service_with_connect_info::<SocketAddr>())
        .with_graceful_shutdown(async move {
            shutdown_signal().await;
            println!("\n🌸 shutting down...");
            shutdown_token.cancel();
        })
        .await?;

    db.close().await;

    Ok(())
}

async fn shutdown_signal() {
    let ctrl_c = tokio::signal::ctrl_c();

    #[cfg(unix)]
    {
        let mut sigterm = tokio::signal::unix::signal(
            tokio::signal::unix::SignalKind::terminate(),
        ).expect("failed to install SIGTERM handler");
        tokio::select! {
            _ = ctrl_c => {},
            _ = sigterm.recv() => {},
        }
    }

    #[cfg(not(unix))]
    {
        ctrl_c.await.ok();
    }
}
