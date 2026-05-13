//! Server-Sent Events endpoint at `/events`.
//!
//! Subscribers (curl, EventSource, the Tauri client) hit this and receive
//! a stream of `event: <name>\ndata: <json>\n\n` blocks for every bunyan
//! lifecycle event. The bus is in-process; the SSE adapter just bridges it.
//!
//! Per-event filtering and per-workspace filtering are left to the client
//! (a 20-line jq pipe). Server-side filters can come later if real load
//! demands them.

use std::convert::Infallible;
use std::sync::Arc;
use std::time::Duration;

use axum::extract::State;
use axum::response::sse::{Event, KeepAlive, Sse};
use futures_util::stream::Stream;
use tokio_stream::wrappers::BroadcastStream;
use tokio_stream::StreamExt;

use crate::state::AppState;

#[utoipa::path(
    get,
    path = "/events",
    responses(
        (status = 200, description = "Server-Sent Events stream of bunyan lifecycle events")
    ),
    tag = "events"
)]
pub async fn stream(
    State(state): State<Arc<AppState>>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let rx = state.event_bus.subscribe();
    let stream = BroadcastStream::new(rx).filter_map(|res| {
        match res {
            Ok(env) => {
                // Use the event name as SSE's `event:` field so EventSource
                // listeners can subscribe to one event type. The data
                // payload is the JSON envelope.
                let event_name = env.event.clone();
                let data = serde_json::to_string(&env).unwrap_or_default();
                Some(Ok(Event::default().event(event_name).data(data)))
            }
            // Broadcast Lagged: SSE clients tolerate gaps; just skip.
            Err(_) => None,
        }
    });
    Sse::new(stream).keep_alive(
        KeepAlive::new()
            .interval(Duration::from_secs(15))
            .text("keep-alive"),
    )
}
