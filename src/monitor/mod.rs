mod websocket;
use crate::{aria2, domain::download::Download};
use std::{
    collections::HashSet,
    sync::mpsc::{self, Receiver, RecvTimeoutError, Sender},
    thread,
    time::Duration,
};

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
        .spawn(move || {
            websocket::websocket_loop(websocket_endpoint, websocket_sender, notifications_tx)
        })
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
