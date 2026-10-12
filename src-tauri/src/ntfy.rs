//! ntfy client: publishes break notifications to the phone and listens for its Done/Skip taps.
//!
//! The topics act as passwords, so nothing here logs a URL or an underlying error message
//! (HTTP client errors can embed the request URL).

use std::io::{BufRead, BufReader};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use microbreaks_core::config::Ntfy;
use microbreaks_core::{control_body, parse_control, Reply};
use serde_json::{json, Value};
use tauri::AppHandle;
use ureq::Agent;

use crate::engine;

/// Each stream is closed and reopened this often. The HTTP client's body timeout is a total
/// budget rather than a per-read one, so this also recovers a connection that died silently
/// (sleep, network change) without relying on keepalives.
const STREAM_LIFETIME: Duration = Duration::from_secs(600);
const MAX_BACKOFF: Duration = Duration::from_secs(60);

fn agent(body_timeout: Option<Duration>) -> Agent {
    Agent::config_builder()
        .timeout_connect(Some(Duration::from_secs(10)))
        .timeout_recv_response(Some(Duration::from_secs(30)))
        .timeout_recv_body(body_timeout)
        .build()
        .into()
}

fn describe(e: &ureq::Error) -> String {
    match e {
        ureq::Error::StatusCode(code) => format!("http status {code}"),
        ureq::Error::Timeout(_) => "timeout".into(),
        _ => "network error".into(),
    }
}

fn root(cfg: &Ntfy) -> &str {
    cfg.server.trim_end_matches('/')
}

/// Sends a movement-break notification with Done and Skip buttons (plus "Going to sleep" in
/// Off-hours) that POST to the control topic. Blocking; call from a worker thread.
pub fn publish(cfg: &Ntfy, id: u64, title: &str, message: &str, sleep_button: bool) {
    let control_url = format!("{}/{}", root(cfg), cfg.control_topic);
    let action = |label: &str, reply: Reply| {
        json!({
            "action": "http", "label": label, "url": control_url, "method": "POST",
            "body": control_body(reply, id), "clear": true,
        })
    };
    let mut actions = vec![action("Done", Reply::Done), action("Skip", Reply::Skip)];
    if sleep_button {
        actions.push(action("Going to sleep", Reply::Sleep));
    }
    let body = json!({
        "topic": cfg.alerts_topic, "title": title, "message": message, "priority": 4,
        "actions": actions,
    });
    let result = agent(Some(Duration::from_secs(30)))
        .post(format!("{}/", root(cfg)))
        .header("Content-Type", "application/json")
        .send(body.to_string());
    if let Err(e) = result {
        eprintln!("microbreaks: ntfy publish failed ({})", describe(&e));
    }
}

/// Starts the control-topic listener on its own thread.
pub fn spawn_subscriber(app: AppHandle, cfg: Ntfy) {
    std::thread::spawn(move || subscribe_forever(app, cfg));
}

fn subscribe_forever(app: AppHandle, cfg: Ntfy) {
    let agent = agent(Some(STREAM_LIFETIME));
    // Replay from the last message seen, or from startup, so taps made while the connection was
    // down still arrive. Stale replays are harmless: replies for a resolved break are ignored.
    let mut since = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs()).to_string();
    let mut backoff = Duration::from_secs(2);
    loop {
        let url = format!("{}/{}/json?since={since}", root(&cfg), cfg.control_topic);
        match agent.get(&url).call() {
            Ok(response) => {
                backoff = Duration::from_secs(2);
                let reader = BufReader::new(response.into_body().into_reader());
                // Ends on EOF, a read error, or the lifetime timeout; all mean "reconnect".
                for line in reader.lines().map_while(Result::ok) {
                    handle_line(&app, &line, &mut since);
                }
                std::thread::sleep(Duration::from_secs(1));
            }
            Err(e) => {
                eprintln!("microbreaks: ntfy subscribe failed ({}), retrying in {}s", describe(&e), backoff.as_secs());
                std::thread::sleep(backoff);
                backoff = (backoff * 2).min(MAX_BACKOFF);
            }
        }
    }
}

/// One JSON line from the stream. Only `message` events matter; `open` and `keepalive` carry nothing.
fn handle_line(app: &AppHandle, line: &str, since: &mut String) {
    let Ok(v) = serde_json::from_str::<Value>(line) else { return };
    if v["event"] != "message" {
        return;
    }
    if let Some(id) = v["id"].as_str() {
        *since = id.to_string();
    }
    if let Some(input) = v["message"].as_str().and_then(parse_control) {
        engine::push(app, input);
        engine::tick(app);
    }
}
