use axum::{
    body::Body,
    extract::State,
    http::{header, Request, StatusCode},
    middleware::Next,
    response::{
        sse::{Event, KeepAlive, Sse},
        Html, IntoResponse, Response,
    },
};
use std::convert::Infallible;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio_stream::wrappers::BroadcastStream;
use tokio_stream::StreamExt;

use super::progress::{MigrationMessage, MigrationProgress};

// ---------------------------------------------------------------------------
// Ready flag — shared between middleware and the migration completion logic
// ---------------------------------------------------------------------------

/// Shared state indicating whether the app is ready (migrations complete).
#[derive(Clone)]
pub struct ReadyFlag(pub Arc<AtomicBool>);

impl ReadyFlag {
    pub fn new(ready: bool) -> Self {
        Self(Arc::new(AtomicBool::new(ready)))
    }

    pub fn is_ready(&self) -> bool {
        self.0.load(Ordering::Acquire)
    }

    pub fn set_ready(&self) {
        self.0.store(true, Ordering::Release);
    }
}

// ---------------------------------------------------------------------------
// Middleware
// ---------------------------------------------------------------------------

/// Axum middleware that intercepts requests during migration.
///
/// - `/health` returns 200 during migration (server is up, accepting connections).
///   This ensures Docker marks the container as healthy so reverse proxies
///   (e.g. Traefik) route traffic to it — allowing users to see the maintenance page.
/// - `/maintenance/events` is always passed through (SSE endpoint).
/// - All other requests return the maintenance HTML page during migration,
///   and pass through after migration completes.
pub async fn maintenance_middleware(
    State(ready): State<ReadyFlag>,
    request: Request<Body>,
    next: Next,
) -> Response {
    if ready.is_ready() {
        return next.run(request).await;
    }

    let path = request.uri().path();

    match path {
        "/maintenance/events" => next.run(request).await,
        "/health" => Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, "text/plain")
            .body(Body::from("migrating"))
            .unwrap()
            .into_response(),
        _ => Html(MAINTENANCE_HTML).into_response(),
    }
}

// ---------------------------------------------------------------------------
// SSE handler
// ---------------------------------------------------------------------------

/// SSE endpoint that streams migration progress events.
/// Replays any messages that were sent before this client connected,
/// then streams live events.
pub async fn migration_events_handler() -> impl IntoResponse {
    let stream: Pin<Box<dyn tokio_stream::Stream<Item = Result<Event, Infallible>> + Send>> =
        if let Some((history, rx)) = MigrationProgress::subscribe() {
            // Replay past messages, then stream live ones
            let history_stream =
                tokio_stream::iter(history.into_iter().map(Ok));
            let live_stream = BroadcastStream::new(rx).filter_map(|result| match result {
                Ok(msg) => Some(Ok(msg)),
                Err(_) => None, // Skip lagged messages
            });

            Box::pin(history_stream.chain(live_stream).map(
                |result: Result<MigrationMessage, Infallible>| {
                    let msg = result.unwrap();
                    let event_name = match &msg {
                        MigrationMessage::Progress { .. } => "progress",
                        MigrationMessage::Error { .. } => "error",
                        MigrationMessage::Complete => "complete",
                        MigrationMessage::Failed { .. } => "failed",
                    };
                    let json = serde_json::to_string(&msg).unwrap_or_default();
                    Ok(Event::default().event(event_name).data(json))
                },
            ))
        } else {
            // No migration running — send a single "complete" event
            Box::pin(tokio_stream::once(Ok(
                Event::default()
                    .event("complete")
                    .data(r#"{"type":"Complete"}"#),
            )))
        };

    Sse::new(stream).keep_alive(KeepAlive::default())
}

// ---------------------------------------------------------------------------
// Maintenance page HTML
// ---------------------------------------------------------------------------

const MAINTENANCE_HTML: &str = r#"<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>FsPulse - Database Maintenance</title>
<style>
  * { margin: 0; padding: 0; box-sizing: border-box; }
  body {
    font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif;
    background: #1a1a2e;
    color: #e0e0e0;
    min-height: 100vh;
    display: flex;
    flex-direction: column;
    align-items: center;
    padding: 2rem;
  }
  h1 { color: #64b5f6; margin-bottom: 0.5rem; font-size: 1.5rem; }
  .subtitle { color: #90a4ae; margin-bottom: 1.5rem; font-size: 0.95rem; }
  .console {
    background: #0d1117;
    border: 1px solid #30363d;
    border-radius: 8px;
    padding: 1rem;
    width: 100%;
    max-width: 800px;
    height: 400px;
    overflow-y: auto;
    font-family: 'Cascadia Code', 'Fira Code', 'Consolas', monospace;
    font-size: 0.85rem;
    line-height: 1.5;
  }
  .console .line { padding: 1px 0; white-space: pre-wrap; word-break: break-all; }
  .console .line.error { color: #f44336; }
  .console .line.success { color: #66bb6a; font-weight: bold; }
  .status {
    margin-top: 1rem;
    padding: 0.75rem 1.5rem;
    border-radius: 6px;
    font-size: 0.9rem;
    text-align: center;
    max-width: 800px;
    width: 100%;
  }
  .status.running { background: #1e3a5f; color: #64b5f6; }
  .status.done { background: #1b4332; color: #66bb6a; }
  .status.failed { background: #4a1a1a; color: #f44336; }
  .spinner { display: inline-block; animation: spin 1s linear infinite; }
  @keyframes spin { to { transform: rotate(360deg); } }
</style>
</head>
<body>
<h1>FsPulse - Database Maintenance</h1>
<p class="subtitle">A schema migration is in progress. This page will refresh automatically when complete.</p>
<div class="console" id="console"></div>
<div class="status running" id="status">
  <span class="spinner">&#9696;</span> Migration in progress...
</div>
<script>
(function() {
  var consoleEl = document.getElementById('console');
  var statusEl = document.getElementById('status');

  function addLine(text, cls) {
    var div = document.createElement('div');
    div.className = 'line' + (cls ? ' ' + cls : '');
    div.textContent = text;
    consoleEl.appendChild(div);
    consoleEl.scrollTop = consoleEl.scrollHeight;
  }

  var es = new EventSource('/maintenance/events');

  es.addEventListener('progress', function(e) {
    var data = JSON.parse(e.data);
    addLine(data.message);
  });

  es.addEventListener('error', function(e) {
    if (e.data) {
      var data = JSON.parse(e.data);
      addLine('ERROR: ' + data.message, 'error');
    }
  });

  es.addEventListener('complete', function() {
    es.close();
    addLine('Migration complete! Loading application...', 'success');
    statusEl.className = 'status done';
    statusEl.textContent = 'Migration complete! Redirecting...';
    setTimeout(function() { window.location.reload(); }, 2000);
  });

  es.addEventListener('failed', function(e) {
    es.close();
    var data = JSON.parse(e.data);
    addLine('FATAL: ' + data.message, 'error');
    addLine('The server will exit. Check logs for details.', 'error');
    statusEl.className = 'status failed';
    statusEl.textContent = 'Migration failed. Check server logs.';
  });

  es.onerror = function() {
    if (es.readyState === EventSource.CLOSED) {
      addLine('Connection to server lost.', 'error');
      statusEl.className = 'status failed';
      statusEl.textContent = 'Connection lost. The server may have stopped.';
    }
  };
})();
</script>
</body>
</html>"#;
