use std::net::SocketAddr;
use std::sync::Arc;

use axum::{
    routing::{get, post},
    http::StatusCode,
    response::IntoResponse,
    Json, Router, Extension,
};
use serde::{Deserialize, Serialize};
use clap::Parser;
use tower_http::trace::TraceLayer;
use tracing_subscriber::filter::EnvFilter;
use tracing_subscriber::prelude::*;
use tracing::{debug, info};

mod payload;
mod webhook;
mod cli;
mod error;

#[tokio::main]
async fn main() {
    let args = Arc::new(cli::Args::parse());

    tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::try_from_default_env().unwrap())
        .with(tracing_subscriber::fmt::layer())
        .init();

    let app = Router::new()
        .route("/", post(webhook::webhook))
        .layer(Extension(args.clone()))
        .layer(TraceLayer::new_for_http());
    let addr = &args.bind_address.unwrap_or(SocketAddr::from(([0, 0, 0, 0], 80)));

    debug!("Listening on http://{}", addr);

    axum::Server::bind(addr)
        .serve(app.into_make_service())
        .await
        .unwrap();
}
