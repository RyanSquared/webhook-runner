//! Documentation of the command options of the crate can be found by running `webhook-runner -h`,
//! including flags, options, and environment variables.

use std::net::SocketAddr;
use std::sync::Arc;

use axum::{
    body,
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
    Extension, Json, Router,
};
use clap::Parser;
use tempdir::TempDir;
use tower_http::trace::TraceLayer;
use tracing::{debug, info};
use tracing_subscriber::filter::{EnvFilter, LevelFilter};
use tracing_subscriber::prelude::*;
use tower::ServiceBuilder;
use tower_http::ServiceBuilderExt;

mod cli;
mod error;
mod payload;
mod signature;
mod status;
mod util;
mod webhook;

fn setup_registry() {
    let envfilter = EnvFilter::builder()
        .with_default_directive(LevelFilter::DEBUG.into())
        .from_env_lossy();
    tracing_subscriber::registry()
        .with(envfilter)
        .with(tracing_subscriber::fmt::layer())
        .init();
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    setup_registry();

    let args = Arc::new(cli::Args::parse());
    args.assert();
    info!("Running with the following options: {:?}", &args);

    let mut gpgdirs: util::KeyringDirs = Default::default();
    if let Some(keyring) = args.commit_keyring() {
        gpgdirs
            .commit
            .replace(util::assert_gpg_directory(keyring.clone().as_str()).await?);
    }
    if let Some(keyring) = args.tag_keyring() {
        gpgdirs
            .tag
            .replace(util::assert_gpg_directory(keyring.clone().as_str()).await?);
    }

    let app = Router::new()
        .route("/", post(webhook::webhook))
        .layer(
            ServiceBuilder::new()
            .map_request_body(body::boxed)
            .layer(axum::middleware::from_fn(signature::HubSignature256::verify_middleware))
        )
        .layer(Extension(args.clone()))
        .layer(Extension(Arc::new(gpgdirs)))
        .layer(TraceLayer::new_for_http());
    let addr = &args.bind_address;

    info!("Listening on http://{}", addr);

    axum::Server::bind(addr)
        .serve(app.into_make_service())
        .await
        .unwrap();

    Ok(())
}
