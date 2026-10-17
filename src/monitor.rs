use std::{
    collections::HashSet,
    sync::mpsc::{self, Receiver, RecvTimeoutError, Sender},
    thread,
    time::Duration,
};

use tungstenite::{Message, stream::MaybeTlsStream};

use crate::{aria2, domain::download::Download};

// Messages delivered from snapshopt to UI
pub enum MonitorUpdate {
    Snapshot(Vec<Download>),
    Progress(Vec<Download>),
    Status(String),
    Heartbeat,
}

// Downlods the full list and converts it into our internal model
fn fetch_snapshot(endpoint: &str, rpc_secret: Option<&str>) -> Result<Vec<Download>, String> {
    let (_, _, _, responses) = aria2::fetch_all_downloads(endpoint, rpc_secret)?;

    responses
        .into_iter()
        .map(|response| response.into_download())
        .collect()
}

fn fetch_active(endpoint: &str, rpc_secret: Option<&str>) -> Result<Vec<Download>, String> {
    let request = aria2::build_tell_active_request(rpc_secret)?;

    aria2::fetch_downloads(endpoint, &request)?
        .into_iter()
        .map(|response| response.into_download())
        .collect()
}

fn active_gid_set(downloads: &[Download]) -> HashSet<String> {
    downloads
        .iter()
        .filter(|download| download.status.is_active())
        .map(|download| download.gid.clone())
        .collect()
}

pub fn start(
    endpoint: String,
    rpc_secret: Option<String>,
    initial_active_gids: Vec<String>,
) -> Result<Receiver<MonitorUpdate>, String> {
    let (updates_tx, updates_rx) = mpsc::channel();
    let (notifications_tx, notifications_rx) = mpsc::channel();
    let websocket_sender = updates_tx.clone();
    let websocket_endpoint = endpoint.clone();

    thread::Builder::new()
        .name("ariawatch-websocket".to_owned())
        .spawn(move || websocket_loop(websocket_endpoint, websocket_sender, notifications_tx))
        .map_err(|error| format!("Could not start websocket listener: {error}"))?;

    thread::Builder::new()
        .name("ariawatch-monitor".to_owned())
        .spawn(move || {
            monitor_loop(
                endpoint,
                rpc_secret,
                initial_active_gids.into_iter().collect(),
                updates_tx,
                notifications_rx,
            );
        })
        .map_err(|error| format!("Could not start monitoring working: {error}"))?;

    Ok(updates_rx)
}

fn websocket_url(endpoint: &str) -> Result<String, String> {
    if let Some(address) = endpoint.strip_prefix("http://") {
        Ok(format!("ws://{address}"))
    } else if let Some(address) = endpoint.strip_prefix("https://") {
        Ok(format!("wss://{address}"))
    } else {
        Err(format!(
            "Cannot derive websocket url from RPC endpoint: {endpoint}"
        ))
    }
}

fn websocket_loop(endpoint: String, updates: Sender<MonitorUpdate>, notifications: Sender<()>) {
    let endpoint = match websocket_url(&endpoint) {
        Ok(endpoint) => endpoint,
        Err(error) => {
            let _ = updates.send(MonitorUpdate::Status(error));
            return;
        }
    };

    loop {
        if updates
            .send(MonitorUpdate::Status(
                "Connecting to aria2 WebSocket".to_owned(),
            ))
            .is_err()
        {
            return;
        }

        match tungstenite::connect(&endpoint) {
            Ok((mut socket, _response)) => {
                if updates
                    .send(MonitorUpdate::Status("WebSocket connected".to_owned()))
                    .is_err()
                {
                    return;
                }

                let timeout_result = match socket.get_mut() {
                    MaybeTlsStream::Plain(stream) => {
                        stream.set_read_timeout(Some(Duration::from_secs(1)))
                    }

                    MaybeTlsStream::Rustls(stream) => {
                        stream.sock.set_read_timeout(Some(Duration::from_secs(1)))
                    }

                    _ => Ok(()),
                };

                if let Err(error) = timeout_result {
                    if updates
                        .send(MonitorUpdate::Status(format!(
                            "Could not configure websocket timeout: {error}",
                        )))
                        .is_err()
                    {
                        return;
                    }

                    thread::sleep(Duration::from_secs(2));
                    continue;
                }

                loop {
                    match socket.read() {
                        Ok(message) => {
                            if updates.send(MonitorUpdate::Heartbeat).is_err() {
                                return;
                            }

                            match message {
                                Message::Text(payload) => {
                                    let Ok(value) =
                                        serde_json::from_str::<serde_json::Value>(payload.as_str())
                                    else {
                                        continue;
                                    };

                                    let Some(method) = value.get("method").and_then(|v| v.as_str())
                                    else {
                                        continue;
                                    };

                                    if !method.starts_with("aria2.onDownload") {
                                        continue;
                                    }

                                    let gid = value
                                        .pointer("/params/0/gid")
                                        .and_then(|v| v.as_str())
                                        .unwrap_or("unknown GID");

                                    if updates
                                        .send(MonitorUpdate::Status(format!("{method} · {gid}")))
                                        .is_err()
                                    {
                                        return;
                                    }

                                    if notifications.send(()).is_err() {
                                        return;
                                    }
                                }

                                Message::Close(_) => break,

                                _ => {}
                            }
                        }

                        Err(tungstenite::Error::Io(error))
                            if matches!(
                                error.kind(),
                                std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                            ) =>
                        {
                            if updates.send(MonitorUpdate::Heartbeat).is_err() {
                                return;
                            }
                        }

                        Err(error) => {
                            if updates
                                .send(MonitorUpdate::Status(format!(
                                    "WebSocket disconnected: {error}"
                                )))
                                .is_err()
                            {
                                return;
                            }

                            break;
                        }
                    }
                }
            }

            Err(error) => {
                if updates
                    .send(MonitorUpdate::Status(format!(
                        "WebSocket connection failed: {error}"
                    )))
                    .is_err()
                {
                    return;
                }
            }
        }

        thread::sleep(Duration::from_secs(2));
    }
}

fn refresh_snapshot(
    endpoint: &str,
    rpc_secret: Option<&str>,
    updates: &Sender<MonitorUpdate>,
    active_gids: &mut HashSet<String>,
) -> bool {
    match fetch_snapshot(endpoint, rpc_secret) {
        Ok(downloads) => {
            *active_gids = active_gid_set(&downloads);
            updates.send(MonitorUpdate::Snapshot(downloads)).is_ok()
        }
        Err(error) => updates
            .send(MonitorUpdate::Status(format!(
                "Snapshot refresh failed: {error}"
            )))
            .is_ok(),
    }
}

fn monitor_loop(
    endpoint: String,
    rpc_secret: Option<String>,
    mut active_gids: HashSet<String>,
    updates: Sender<MonitorUpdate>,
    notifications: Receiver<()>,
) {
    loop {
        let idle = active_gids.is_empty();

        let poll_interval = if active_gids.is_empty() {
            Duration::from_secs(5)
        } else {
            Duration::from_secs(1)
        };

        match notifications.recv_timeout(poll_interval) {
            Ok(()) => {
                if !refresh_snapshot(&endpoint, rpc_secret.as_deref(), &updates, &mut active_gids) {
                    return;
                }
                continue;
            }

            Err(RecvTimeoutError::Timeout) => {}

            Err(RecvTimeoutError::Disconnected) => {
                thread::sleep(poll_interval);
            }
        }

        if idle {
            if !refresh_snapshot(&endpoint, rpc_secret.as_deref(), &updates, &mut active_gids) {
                return;
            }

            continue;
        }

        let downloads = match fetch_active(&endpoint, rpc_secret.as_deref()) {
            Ok(downloads) => downloads,
            Err(error) => {
                if updates
                    .send(MonitorUpdate::Status(format!(
                        "Progress refresh failed: {error}"
                    )))
                    .is_err()
                {
                    return;
                }

                continue;
            }
        };

        let current_gids = active_gid_set(&downloads);

        if current_gids != active_gids {
            if !refresh_snapshot(&endpoint, rpc_secret.as_deref(), &updates, &mut active_gids) {
                return;
            }
            continue;
        }

        if updates.send(MonitorUpdate::Progress(downloads)).is_err() {
            return;
        }
    }
}
