use std::{
    fs,
    path::{Path, PathBuf},
    sync::{
        Arc,
        mpsc::{self, Receiver, Sender, TryRecvError},
    },
    thread,
};

use crate::{backend::DownloadBackend, link_file::parse_link_file, logging};

enum LinkFileWorkerCommand {
    ImportFile(PathBuf),
}

pub(super) enum LinkFileWorkerEvent {
    Imported {
        total: usize,
        added: usize,
        failed: usize,
    },
    Failed(String),
}

#[derive(Debug, PartialEq, Eq)]
struct BatchImportReport {
    total: usize,
    added: usize,
    failed: usize,
}

pub(super) struct LinkFileWorker {
    commands: Sender<LinkFileWorkerCommand>,
    events: Receiver<LinkFileWorkerEvent>,
}

impl LinkFileWorker {
    pub(super) fn start(backend: Arc<dyn DownloadBackend>) -> Result<Self, String> {
        let (command_sender, command_receiver) = mpsc::channel();
        let (event_sender, event_receiver) = mpsc::channel();

        thread::Builder::new()
            .name(String::from("ariawatch-link-file-watcher"))
            .spawn(move || {
                while let Ok(command) = command_receiver.recv() {
                    let event = match command {
                        LinkFileWorkerCommand::ImportFile(path) => {
                            match import_link_file(backend.as_ref(), &path) {
                                Ok(report) => LinkFileWorkerEvent::Imported {
                                    total: report.total,
                                    added: report.added,
                                    failed: report.failed,
                                },
                                Err(error) => LinkFileWorkerEvent::Failed(error),
                            }
                        }
                    };

                    if event_sender.send(event).is_err() {
                        break;
                    }
                }
            })
            .map_err(|error| format!("Could not start link-file worker: {error}"))?;

        Ok(Self {
            commands: command_sender,
            events: event_receiver,
        })
    }

    // Queues a link-file import without blocking the TUI
    pub(super) fn import_file(&self, path: PathBuf) -> Result<(), String> {
        self.commands
            .send(LinkFileWorkerCommand::ImportFile(path))
            .map_err(|_| String::from("Link-file worker is unavailable"))
    }

    pub(super) fn try_recv(&self) -> Result<LinkFileWorkerEvent, TryRecvError> {
        self.events.try_recv()
    }
}

// Reads and validates the file before submitting any downloads
fn import_link_file(
    backend: &dyn DownloadBackend,
    path: &Path,
) -> Result<BatchImportReport, String> {
    let contents = fs::read_to_string(path)
        .map_err(|error| format!("COuld not read link file {}: {error}", path.display()))?;
    let links = parse_link_file(&contents)?;
    Ok(submit_links(&links, |uri| backend.add_uri(uri)))
}

// submits links independently, continuing when an individual submission fails
fn submit_links(
    links: &[String],
    mut add_uri: impl FnMut(&str) -> Result<String, String>,
) -> BatchImportReport {
    let mut report = BatchImportReport {
        total: links.len(),
        added: 0,
        failed: 0,
    };

    for (index, link) in links.iter().enumerate() {
        match add_uri(link) {
            Ok(_gid) => report.added += 1,
            Err(error) => {
                report.failed += 1;
                logging::error(&format!(
                    "Batch link {} failed to submit: {error}",
                    index + 1
                ));
            }
        }
    }

    report
}

#[cfg(test)]
mod tests {
    use super::{BatchImportReport, submit_links};

    #[test]
    fn continues_importing_after_an_individual_link_fails() {
        let links = vec![
            String::from("https://example.com/first.zip"),
            String::from("https://example.com/fails.zip"),
            String::from("https://example.com/last.zip"),
        ];

        let mut attempted = Vec::new();

        let report = submit_links(&links, |uri| {
            attempted.push(uri.to_owned());

            if uri.contains("fails") {
                Err(String::from("simulated failure"))
            } else {
                Ok(String::from("0123456789abcdef"))
            }
        });

        assert_eq!(
            report,
            BatchImportReport {
                total: 3,
                added: 2,
                failed: 1,
            }
        );

        // A failed entry must not prevent later entries from being submitted.
        assert_eq!(attempted, links);
    }

    #[test]
    fn reports_success_when_all_links_are_submitted() {
        let links = vec![
            String::from("https://example.com/first.zip"),
            String::from("https://example.com/second.zip"),
        ];

        let report = submit_links(&links, |_| Ok(String::from("0123456789abcdef")));

        assert_eq!(
            report,
            BatchImportReport {
                total: 2,
                added: 2,
                failed: 0,
            }
        );
    }
}
