use std::{sync::mpsc::Sender, thread, time::Duration};

use tungstenite::{Message, stream::MaybeTlsStream};

use super::MonitorUpdate;

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

fn parse_lifecycle_notification(payload: &str) -> Option<(String, String)> {
    let value = serde_json::from_str::<serde_json::Value>(payload).ok()?;
    let method = value.get("method")?.as_str()?;

    if !method.starts_with("aria2.onDownload") {
        return None;
    }

    let gid = value
        .pointer("/params/0/gid")
        .and_then(|value| value.as_str())
        .unwrap_or("unknown GID");

    Some((method.to_owned(), gid.to_owned()))
}

#[derive(Debug, PartialEq, Eq)]
enum WebSocketMessageAction {
    Continue,
    Reconnect,
    Shutdown,
}

fn handle_websocket_message(
    message: Message,
    updates: &Sender<MonitorUpdate>,
    notifications: &Sender<()>,
) -> WebSocketMessageAction {
    if updates.send(MonitorUpdate::Heartbeat).is_err() {
        return WebSocketMessageAction::Shutdown;
    }

    match message {
        Message::Text(payload) => {
            let Some((method, gid)) = parse_lifecycle_notification(payload.as_str()) else {
                return WebSocketMessageAction::Continue;
            };

            if updates
                .send(MonitorUpdate::Status(format!("{method} · {gid}")))
                .is_err()
            {
                return WebSocketMessageAction::Shutdown;
            }

            if notifications.send(()).is_err() {
                return WebSocketMessageAction::Shutdown;
            }

            WebSocketMessageAction::Continue
        }

        Message::Close(_) => WebSocketMessageAction::Reconnect,

        _ => WebSocketMessageAction::Continue,
    }
}

pub(super) fn websocket_loop(
    endpoint: String,
    updates: Sender<MonitorUpdate>,
    notifications: Sender<()>,
) {
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

                if notifications.send(()).is_err() {
                    return;
                }

                loop {
                    match socket.read() {
                        Ok(message) => {
                            match handle_websocket_message(message, &updates, &notifications) {
                                WebSocketMessageAction::Continue => {}
                                WebSocketMessageAction::Reconnect => break,
                                WebSocketMessageAction::Shutdown => return,
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
