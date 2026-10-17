use std::{
    collections::HashSet,
    sync::mpsc::{self, Receiver, Sender},
    thread,
    time::Duration,
};

use crate::{aria2, domain::download::Download};

// Messages delivered from snapshopt to UI
pub enum MonitorUpdate {
    Snapshot(Vec<Download>),
    Progress(Vec<Download>),
    Status(String),
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
    let (sender, receiver) = mpsc::channel();

    thread::Builder::new()
        .name("ariawatch-monitor".to_owned())
        .spawn(move || {
            monitor_loop(
                endpoint,
                rpc_secret,
                initial_active_gids.into_iter().collect(),
                sender,
            );
        })
        .map_err(|error| format!("Could not start monitoring working: {error}"))?;

    Ok(receiver)
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
) {
    loop {
        let idle = active_gids.is_empty();

        thread::sleep(if idle {
            Duration::from_secs(5)
        } else {
            Duration::from_secs(1)
        });

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
