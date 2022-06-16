use axum::{
    response::IntoResponse,
    Json,
};

use crate::payload::Payload;

/// Receive a webhook from a GitHub server indicating a change in code, match upon an event, and
/// dispatch the JSON blob to a configured script.
pub async fn webhook(Json(payload): Json<Payload>) -> impl IntoResponse {
    // TODO(RyanSquared): Tracing does not log here. I am unsure why.
    tracing::info!("hello!");
    axum::response::Html::<&'static str>("hi")
}
