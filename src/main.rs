#![forbid(unsafe_code)]
#![forbid(clippy::allow_attributes)]
#![deny(clippy::pedantic)]

use anyhow::Result;
use axum::extract::State;
use axum::{http::StatusCode, routing::get, Json, Router};
use home;
use postgresql_embedded::{PostgreSQL, Settings, VersionReq};
use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;
use std::env;
use std::path::Path;
use std::time::Duration;
use tokio::signal;
use tokio::net::TcpListener;
use tracing::info;

fn is_truthy(str: String) -> bool {
    str == "1" || str == "true"
}

/// Example of how to use postgresql embedded with axum.
#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt().compact().init();
    let args: Vec<String> = env::args().collect();
    let count = args.len() - 1;
    
    if count != 0 && args[1] == "--version" {
        const VERSION: &str = env!("CARGO_PKG_VERSION");
        info!("epg v{VERSION}");
        return Ok(());
    }
    
    let pg_version = "16.4.0";
    
    let port_str = env::var("PGPORT").unwrap_or_else(|_| "5016".into() );
    let pg_port = u16::from_str_radix(&port_str, 10).unwrap();
    let port = pg_port + 3000;
    
    let default_install_dir = match home::home_dir() {
        Some(path) => format!("{}/.theseus/postgresql", path.display()),
        None => "target/epg/install".into(),
    };
    let install_dir = env::var("PGDIR").unwrap_or_else(|_| default_install_dir );
    
    let data_dir = env::var("PGDATA").unwrap_or_else(|_| "target/epg/data".into() );
    let username = env::var("POSTGRES_USER").unwrap_or_else(|_| "postgres".into() );
    let password = env::var("POSTGRES_PASSWORD").unwrap_or_else(|_| "root_pw".into() );
    
    let settings = Settings {
        version: VersionReq::parse(&format!("={pg_version}"))?,
        installation_dir: (&install_dir).into(),
        data_dir: data_dir.into(),
        port: u16::from_str_radix(&port_str, 10).unwrap(),
        temporary: false,
        username: username.into(),
        password: password.into(),
        ..Default::default()
    };
    
    if !Path::new(&format!("{install_dir}/{pg_version}")).exists() {
        info!("Installing PostgreSQL ...");
    }

    let mut postgresql = PostgreSQL::new(settings);
    postgresql.setup().await?;
    
    let with_extensions = env::var("WITH_EXTENSIONS").is_ok_and(is_truthy);
    
    if with_extensions {
        info!("Installing the vector extension from PortalCorp");
        postgresql_extensions::install(
            postgresql.settings(),
            "portal-corp",
            "pgvector_compiled",
            &VersionReq::parse("=0.16.12")?,
        )
        .await?;
    }

    info!("Starting PostgreSQL");
    postgresql.start().await?;
    
    let database_name = if count != 0 { &args[1] } else { "postgres" };
    for i in 0..count {
        let exists = postgresql.database_exists(&args[i + 1]).await?;
        if !exists {
            info!("Creating database: {}", &args[i + 1]);
            postgresql.create_database(&args[i + 1]).await?;
        }
    }
    
    let database_url = postgresql.settings().url(database_name);
    if with_extensions {
        info!("Configuring extension");
        let pool = PgPool::connect(database_url.as_str()).await?;
        pool.close().await;

        info!("Restarting database");
        postgresql.stop().await?;
        postgresql.start().await?;
    }
    
    let pool = PgPoolOptions::new()
        .max_connections(5)
        .acquire_timeout(Duration::from_secs(3))
        .connect(&database_url)
        .await?;
    
    if with_extensions {
        info!("Enabling extension");
        enable_extension(&pool).await?;
    }
    
    let bind = format!("0.0.0.0:{port}");
    let app = Router::new().route("/", get(extensions)).with_state(pool);
    let listener = TcpListener::bind(bind).await.unwrap();
    info!("Listening on {}", listener.local_addr()?);
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;
    //info!("Shutting down...");
    postgresql.stop().await?;

    Ok(())
}

async fn shutdown_signal() {
    let ctrl_c = async {
        signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("failed to install signal handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();
    
    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }
}

async fn enable_extension(pool: &PgPool) -> Result<()> {
    sqlx::query("CREATE EXTENSION IF NOT EXISTS vector")
        .execute(pool)
        .await?;
    Ok(())
}

async fn extensions(State(pool): State<PgPool>) -> Result<Json<Vec<String>>, (StatusCode, String)> {
    sqlx::query_scalar("SELECT name FROM pg_available_extensions ORDER BY name")
        .fetch_all(&pool)
        .await
        .map(Json)
        .map_err(internal_error)
}

fn internal_error<E: std::error::Error>(err: E) -> (StatusCode, String) {
    (StatusCode::INTERNAL_SERVER_ERROR, err.to_string())
}
