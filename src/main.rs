//! Documentation of the command options of the crate can be found by running `webhook-runner -h`,
//! including flags, options, and environment variables.

use std::net::SocketAddr;
use std::sync::Arc;

use axum::{
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
    Extension, Json, Router,
};
use clap::Parser;
use tower_http::trace::TraceLayer;
use tracing::{debug, info};
use tracing_subscriber::filter::{EnvFilter, LevelFilter};
use tracing_subscriber::prelude::*;

mod cli;
mod error;
mod payload;
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
async fn main() {
    setup_registry();

    let args = Arc::new(cli::Args::parse().assert());
    info!("Running with the following options: {:?}", &args);

    let app = Router::new()
        .route("/", post(webhook::webhook))
        .layer(Extension(args.clone()))
        .layer(TraceLayer::new_for_http());
    let addr = &args.bind_address;

    info!("Listening on http://{}", addr);

    axum::Server::bind(addr)
        .serve(app.into_make_service())
        .await
        .unwrap();
}
