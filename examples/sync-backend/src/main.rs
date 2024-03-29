//! Example data synchronization backend
//!
//! ** Strictly for the demo purposes only **
//! ** Known issues **:
//! - It works only for single database
//!
//! Run with
//!
//! ```not_rust
//! cd examples && cargo run -p sync-backend
//! ```

use axum::{
    extract::{BodyStream, Path, State, Query},
    http::StatusCode,
    body,
    response,
    routing::get,
    Router, Server,
};
use futures::StreamExt;
use journal::{Journal, AsyncReadJournalStream, AsyncWriteJournalStream};
use tokio::io::AsyncWriteExt;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};
use serde::Deserialize;

fn to_error<T: std::fmt::Debug>(_e: T) -> StatusCode {
    StatusCode::INTERNAL_SERVER_ERROR
}

#[derive(Debug, Default, Deserialize)]
#[allow(dead_code)]
struct Params {
    #[serde(rename="snapshot-id")]
    snapshot_id: u64,
}

/// post new journal snapshots
async fn post_snapshot(
    State(state): State<AppState>,
    Path(_domain): Path<String>,
    mut stream: BodyStream,
) -> Result<&'static str, StatusCode> {
    let mut write_stream = AsyncWriteJournalStream::new(state.journal_path).spawn();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(to_error)?;
        write_stream.write_all(&chunk).await.map_err(to_error)?;
    };
    Ok("OK")
}

/// get latest knowns snapshot num
async fn head_snapshot(
    State(state): State<AppState>,
    Path(_domain): Path<String>,
) -> Result<impl response::IntoResponse, StatusCode> {
    let res = tokio::task::spawn_blocking(move ||{
        let journal = Journal::try_from(state.journal_path)
            .or_else(|_e| Journal::create(state.journal_path))?;
        Ok::<_, journal::Error>(journal.get_header().snapshot_counter)
    });
    let snapshot_id = res.await.map_err(to_error)?.map_err(to_error)?;
    let headers = response::AppendHeaders([("x-snapshot-id", snapshot_id.to_string())]);
    Ok((headers, "head"))
}

/// get new snapshots
async fn get_snapshot(
    State(state): State<AppState>,
    Path(_domain): Path<String>,
    params: Option<Query<Params>>,
) -> Result<impl response::IntoResponse, StatusCode> {
    let stream = AsyncReadJournalStream::new(
        state.journal_path,
        params.map(|p| p.snapshot_id).unwrap_or(0)
    ).spawn();
    Ok(body::StreamBody::new(tokio_util::io::ReaderStream::new(stream)))
}

#[derive(Debug, Clone)]
struct AppState {
    journal_path: &'static str
}

impl AppState {
    fn new() -> Self {
        Self {
            journal_path: "/tmp/journal"
        }
    }
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::new(
            std::env::var("RUST_LOG")
                .unwrap_or_else(|_| "sync_backend=debug,tower_http=debug".into()),
        ))
        .with(tracing_subscriber::fmt::layer())
        .init();

    let app = Router::new()
        .route("/domain/:domain", get(get_snapshot).head(head_snapshot).post(post_snapshot))
        .with_state(AppState::new());

    let addr = std::net::SocketAddr::from(([0, 0, 0, 0], 8080));
    tracing::debug!("listening on {:?}", addr);
    Server::bind(&addr)
        .serve(app.into_make_service())
        .await
        .unwrap()
}
