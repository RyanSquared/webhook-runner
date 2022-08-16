//! Documentation of the command options of the crate can be found by running `webhook-runner -h`,
//! including flags, options, and environment variables.

use std::sync::Arc;

use axum::{body, routing::post, Extension, Router};
use clap::Parser;
use tower::ServiceBuilder;
use tower_http::trace::TraceLayer;
use tower_http::ServiceBuilderExt;
use tracing::info;
use tracing_subscriber::filter::{EnvFilter, LevelFilter};
use tracing_subscriber::prelude::*;

use webhook_runner_lib::cert_builder as cert_builder;
use webhook_runner_lib::repository as repository;
use webhook_runner_lib::KeyringFiles;

mod cli;
mod error;
mod payload;
mod signature;
mod status;
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

    let mut keyrings: KeyringFiles = Default::default();
    if let Some(keyring) = args.commit_keyring() {
        keyrings
            .commit
            .replace(cert_builder::KeyringFile::from_path(keyring.clone().as_str())?);
    }
    if let Some(keyring) = args.tag_keyring() {
        keyrings
            .commit
            .replace(cert_builder::KeyringFile::from_path(keyring.clone().as_str())?);
    }

    let app = Router::new()
        .route("/", post(webhook::webhook))
        .layer(ServiceBuilder::new().map_request_body(body::boxed).layer(
            axum::middleware::from_fn(signature::HubSignature256::verify_middleware),
        ))
        .layer(Extension(args.clone()))
        .layer(Extension(Arc::new(keyrings)))
        .layer(TraceLayer::new_for_http());
    let addr = &args.bind_address;

    info!("Listening on http://{}", addr);

    axum::Server::bind(addr)
        .serve(app.into_make_service())
        .await
        .unwrap();

    Ok(())
}
