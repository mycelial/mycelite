//! Example data synchronization backend
//!
//! ** Strictly for the demo purposes only **
//! ** Known issues **:
//! - It works only for single database and single client
//! - There is no Sync procotol implemented here at all – direct stream of journal.
//! - The server assumes the client sends only new snapshots so the local version is not checked, and it's
//! possible to write the same snapshots multiple times.
//! - No sanity checks that the client actually sends valid data not random garbage
//! - Calling Journal::add_page directly is a hack and rewrites snapshot timestamps/numbers.
//! - The Journal API doesn't allow to write headers directly (yet).
//! - The Journal is experimental, it only supports blocking IO so the scheduler is blocked on the journal IO ops.
//!
//! Run with
//!
//! ```not_rust
//! cd examples && cargo run -p sync-backend
//! ```

use axum::{
    extract::{BodyStream, Path, State, Query},
    response,
    routing::{get, head, post},
    Router, Server,
};
use futures::StreamExt;
use journal::{Journal, Protocol, Stream};
use serde_sqlite::{se, de};
use std::io::Read;
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};
use serde::Deserialize;

fn to_error<T: std::fmt::Debug>(e: T) -> String {
    format!("{:?}", e)
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
    mut stream: BodyStream,
) -> Result<&'static str, String> {
    let mut whole_body = vec![];
    while let Some(chunk) = stream.next().await {
        whole_body.extend(chunk.map_err(to_error)?);
    }
    if whole_body.is_empty() {
        return Ok("");
    }
    let mut whole_body = std::io::Cursor::new(whole_body);
    let mut journal = state.journal.lock().await;

    loop {
        match de::from_reader::<Protocol, _>(&mut whole_body) {
            Ok(Protocol::SnapshotHeader(snapshot_header)) => {
                journal.commit().map_err(to_error)?;
                journal.add_snapshot(&snapshot_header).map_err(to_error)?;
            }
            Ok(Protocol::PageHeader(page_header)) => {
                let mut page = vec![0; page_header.page_size as usize];
                whole_body.read_exact(page.as_mut_slice()).map_err(to_error)?;
                journal.add_page(&page_header, page.as_slice()).map_err(to_error)?;
            },
            Ok(Protocol::EndOfStream(_)) => {
                journal.commit().map_err(to_error)?;
                break;
            },
            Err(e) => return Err(to_error(e)),
        }
    }
    Ok("OK")
}

/// get latest knowns snapshot num
async fn head_snapshot(
    State(state): State<AppState>,
) -> Result<impl response::IntoResponse, String> {
    let journal = state.journal.lock().await;
    let snapshot_id = match journal.current_snapshot() {
        Some(v) => format!("{}", v),
        None => "".into(),
    };
    let headers = response::AppendHeaders([("x-snapshot-id", snapshot_id)]);
    Ok((headers, "head"))
}

/// get new snapshots
async fn get_snapshot(
    State(state): State<AppState>,
    params: Option<Query<Params>>,
    snapshot_id: Option<Path<u64>>,
) -> Result<impl response::IntoResponse, String> {
    let snapshot_id: u64 = params.unwrap_or_default().snapshot_id;
    let mut journal = state.journal.lock().await;
    let iter = journal
        .into_iter()
        .skip_snapshots(snapshot_id);
    let mut buf = vec![];
    Stream::new(iter).read_to_end(&mut buf).map_err(to_error)?;
    Ok(buf)
}

#[derive(Debug, Clone)]
struct AppState {
    journal: Arc<Mutex<journal::Journal>>,
}

impl AppState {
    fn new() -> Self {
        let journal_path = "/tmp/journal";
        let journal = Journal::try_from(journal_path)
            .or_else(|_e| Journal::create(&journal_path))
            .unwrap();
        Self {
            journal: Arc::new(Mutex::new(journal)),
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
        .route("/api/v0/snapshots", get(get_snapshot).head(head_snapshot).post(post_snapshot))
        .with_state(AppState::new());

    let addr = std::net::SocketAddr::from(([0, 0, 0, 0], 8080));
    tracing::debug!("listening on {:?}", addr);
    Server::bind(&addr)
        .serve(app.into_make_service())
        .await
        .unwrap()
}
