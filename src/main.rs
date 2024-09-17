#![forbid(unsafe_code)]
#![forbid(clippy::allow_attributes)]
#![deny(clippy::pedantic)]

use anyhow::Result;
use axum::extract::State;
use axum::{http::StatusCode, routing::get, Json, Router};
use postgresql_embedded::{PostgreSQL, Settings, VersionReq};
use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;
use std::env;
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

    //let db_url =
    //    env::var("DATABASE_URL").unwrap_or_else(|_| "postgresql://postgres:root_pw@localhost:5016".to_string());
    info!("Installing PostgreSQL");
    //let settings = Settings::from_url(&db_url)?;
    let settings = Settings {
        version: VersionReq::parse("=16.4.0")?,
        installation_dir: "target/install".into(),
        data_dir: "target/data".into(),
        port: 5016,
        temporary: false,
        username: "postgres".into(),
        password: "root_pw".into(),
        ..Default::default()
    };
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

    let database_name = "axum-test";
    let exists = postgresql.database_exists(database_name).await?;
    if !exists {
        info!("Creating database {database_name}");
        postgresql.create_database(database_name).await?;
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

    info!("Setup connection pool @ {database_url}");
    let pool = PgPoolOptions::new()
        .max_connections(5)
        .acquire_timeout(Duration::from_secs(3))
        .connect(&database_url)
        .await?;
    
    if with_extensions {
        info!("Enabling extension");
        enable_extension(&pool).await?;
    }
    
    info!("Start application");
    let app = Router::new().route("/", get(extensions)).with_state(pool);

    let listener = TcpListener::bind("0.0.0.0:3030").await.unwrap();
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
