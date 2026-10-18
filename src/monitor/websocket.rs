use super::MonitorUpdate;
use std::{net::TcpStream, sync::mpsc::Sender, thread, time::Duration};
use tungstenite::{Message, WebSocket, stream::MaybeTlsStream};

#[derive(Debug, PartialEq, Eq)]
enum WebSocketMessageAction {
    Continue,
    Reconnect,
    Shutdown,
}

#[derive(Debug, PartialEq, Eq)]
enum WebSocketSessionAction {
    Reconnect,
    Shutdown,
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

fn configure_read_timeout(
    socket: &mut WebSocket<MaybeTlsStream<TcpStream>>,
) -> std::io::Result<()> {
    match socket.get_mut() {
        MaybeTlsStream::Plain(stream) => stream.set_read_timeout(Some(Duration::from_secs(1))),

        MaybeTlsStream::Rustls(stream) => {
            stream.sock.set_read_timeout(Some(Duration::from_secs(1)))
        }

        _ => Ok(()),
    }
}

fn run_websocket_session(
    socket: &mut WebSocket<MaybeTlsStream<TcpStream>>,
    updates: &Sender<MonitorUpdate>,
    notifications: &Sender<()>,
) -> WebSocketSessionAction {
    loop {
        match socket.read() {
            Ok(message) => match handle_websocket_message(message, updates, notifications) {
                WebSocketMessageAction::Continue => {}
                WebSocketMessageAction::Reconnect => {
                    return WebSocketSessionAction::Reconnect;
                }
                WebSocketMessageAction::Shutdown => {
                    return WebSocketSessionAction::Shutdown;
                }
            },

            Err(tungstenite::Error::Io(error))
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                ) =>
            {
                if updates.send(MonitorUpdate::Heartbeat).is_err() {
                    return WebSocketSessionAction::Shutdown;
                }
            }

            Err(error) => {
                if updates
                    .send(MonitorUpdate::Status(format!(
                        "Websocket disconnected: {error}"
                    )))
                    .is_err()
                {
                    return WebSocketSessionAction::Shutdown;
                }

                return WebSocketSessionAction::Reconnect;
            }
        }
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

                if let Err(error) = configure_read_timeout(&mut socket) {
                    if updates
                        .send(MonitorUpdate::Status(format!(
                            "Could not configure websocket timeout: {error}"
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

                match run_websocket_session(&mut socket, &updates, &notifications) {
                    WebSocketSessionAction::Reconnect => {}
                    WebSocketSessionAction::Shutdown => return,
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

#[cfg(test)]
mod tests {
    use super::{parse_lifecycle_notification, websocket_url};

    #[test]
    fn converts_http_to_ws() {
        assert_eq!(
            websocket_url("http://127.0.0.1:6800/jsonrpc").unwrap(),
            "ws://127.0.0.1:6800/jsonrpc"
        );
    }

    #[test]
    fn converts_https_to_wss() {
        assert_eq!(
            websocket_url("https://example.com/jsonrpc").unwrap(),
            "wss://example.com/jsonrpc"
        );
    }

    #[test]
    fn preserves_the_rpc_path() {
        assert_eq!(
            websocket_url("http://localhost:6800/custom/rpc").unwrap(),
            "ws://localhost:6800/custom/rpc"
        );
    }

    #[test]
    fn rejects_unsupported_schemes() {
        assert!(websocket_url("ftp://localhost:6800/jsonrpc").is_err());
    }

    #[test]
    fn parses_lifecycle_notification() {
        let payload = r#"{
            "jsonrpc": "2.0",
            "method": "aria2.onDownloadPause",
            "params": [{"gid": "abc123"}]
        }"#;

        assert_eq!(
            parse_lifecycle_notification(payload),
            Some(("aria2.onDownloadPause".to_owned(), "abc123".to_owned()))
        );
    }

    #[test]
    fn ignores_unrelated_methods() {
        let payload = r#"{"method":"aria2.getVersion","params":[]}"#;

        assert_eq!(parse_lifecycle_notification(payload), None);
    }

    #[test]
    fn ignores_malformed_json() {
        assert_eq!(parse_lifecycle_notification("{invalid"), None);
    }

    #[test]
    fn handles_missing_gid() {
        let payload = r#"{"method":"aria2.onDownloadPause","params":[{}]}"#;

        assert_eq!(
            parse_lifecycle_notification(payload),
            Some(("aria2.onDownloadPause".to_owned(), "unknown GID".to_owned()))
        );
    }
}
