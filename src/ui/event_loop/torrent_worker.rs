use crate::{aria2::TorrentFile, backend::DownloadBackend, domain::status::Status, logging};
use std::{
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, Sender, TryRecvError},
    },
    thread,
    time::Duration,
};

// this enum describes the source of torrent metadata (magent or .torrent file)
pub(super) enum TorrentSource {
    Magnet(String),
    LocalFile(PathBuf),
}

// this is the enum of the msgs sent from the worker back to the TUI event loop
pub(super) enum TorrentWorkerEvent {
    MetadataStarted {
        metadata_gid: String,
    },
    Prepared {
        gid: String,
        files: Vec<TorrentFile>,
    },
    Failed(String),
}

// this one owns the comms channel and background thread (yeah the processing and fetching of the
// metadata and everything is a separate thread from the UI)
pub(super) struct TorrentWorker {
    commands: Sender<TorrentSource>,
    events: Receiver<TorrentWorkerEvent>,
    busy: Arc<AtomicBool>,
    shutdown: Arc<AtomicBool>,
}

impl TorrentWorker {
    // starts a dedicated separate thread and gives it shared access to the backend
    pub(super) fn start(backend: Arc<dyn DownloadBackend>) -> Result<Self, String> {
        let (command_sender, command_receiver) = mpsc::channel();
        let (event_sender, event_receiver) = mpsc::channel();

        let busy = Arc::new(AtomicBool::new(false));
        let shutdown = Arc::new(AtomicBool::new(false));

        let worker_busy = Arc::clone(&busy);
        let worker_shutdown = Arc::clone(&shutdown);

        let _thread = thread::Builder::new()
            .name(String::from("ariawatch-torrent-worker"))
            .spawn(move || {
                while let Ok(source) = command_receiver.recv() {
                    if worker_shutdown.load(Ordering::Acquire) {
                        break;
                    }

                    let result =
                        prepare_torrent(backend.as_ref(), source, &event_sender, &worker_shutdown);

                    let event = match result {
                        Some(Ok((gid, files))) => TorrentWorkerEvent::Prepared { gid, files },
                        Some(Err(error)) => TorrentWorkerEvent::Failed(error),
                        None => break,
                    };

                    if event_sender.send(event).is_err() {
                        break;
                    }

                    worker_busy.store(false, Ordering::Release);
                }
            })
            .map_err(|error| format!("Could not start torrent worker: {error}"))?;

        Ok(Self {
            commands: command_sender,
            events: event_receiver,
            busy,
            shutdown,
        })
    }

    // this lil guy here queues a torrent-prep request for the background thread
    pub(super) fn prepare(&self, source: TorrentSource) -> Result<(), String> {
        self.busy
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .map_err(|_| String::from("A torrent is already being prepared"))?;

        if self.commands.send(source).is_err() {
            self.busy.store(false, Ordering::Release);
            return Err(String::from("Torrent worker is unavailable"));
        }

        Ok(())
    }

    // This one reads a completed worker message without blocking the TUI
    pub(super) fn try_recv(&self) -> Result<TorrentWorkerEvent, TryRecvError> {
        self.events.try_recv()
    }
}

impl Drop for TorrentWorker {
    fn drop(&mut self) {
        self.shutdown.store(true, Ordering::Release);
    }
}

// this is one of the big dawgs. it performs the actual torrent prep in the background thread.
fn prepare_torrent(
    backend: &dyn DownloadBackend,
    source: TorrentSource,
    event_sender: &Sender<TorrentWorkerEvent>,
    shutdown: &AtomicBool,
) -> Option<Result<(String, Vec<TorrentFile>), String>> {
    let gid = match source {
        TorrentSource::Magnet(uri) => {
            let metadata_gid = match backend.add_magnet_for_selection(&uri) {
                Ok(gid) => gid,
                Err(error) => return Some(Err(error)),
            };

            if event_sender
                .send(TorrentWorkerEvent::MetadataStarted {
                    metadata_gid: metadata_gid.clone(),
                })
                .is_err()
            {
                return None;
            }

            loop {
                if shutdown.load(Ordering::Acquire) {
                    return None;
                }

                let metadata = match backend.get_torrent_metadata_status(&metadata_gid) {
                    Ok(metadata) => metadata,
                    Err(error) => return Some(Err(error)),
                };

                if metadata.status == "complete"
                    && let Some(torrent_gid) = metadata.followed_by.first().cloned()
                {
                    if let Err(error) = backend.remove_download(&metadata_gid, &Status::Complete) {
                        logging::warn(&format!(
                            "Could not remove temporary metadata entry {metadata_gid}: {error}"
                        ));
                    }

                    break torrent_gid;
                }

                if metadata.status == "error" || metadata.status == "complete" {
                    let detail = metadata.error_message.or_else(|| {
                        metadata
                            .error_code
                            .map(|code| format!("aria2 reported metadata error code {code}"))
                    });

                    return Some(Err(detail.unwrap_or_else(|| {
                        String::from("Magnet metadata finished without creating a torrent download")
                    })));
                }

                thread::sleep(Duration::from_millis(500));
            }
        }

        TorrentSource::LocalFile(path) => match backend.add_paused_torrent_file(&path) {
            Ok(gid) => gid,
            Err(error) => return Some(Err(error)),
        },
    };

    let files = match backend.get_torrent_files(&gid) {
        Ok(files) => files,
        Err(error) => return Some(Err(error)),
    };

    if files.is_empty() {
        return Some(Err(String::from(
            "aria2 returned an empty torrent file list",
        )));
    }

    Some(Ok((gid, files)))
}
