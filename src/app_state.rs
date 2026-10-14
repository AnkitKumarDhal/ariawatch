use crate::domain::download::{self, Download};

pub struct AppState {
    downloads: Vec<Download>,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            downloads: Vec::new(),
        }
    }

    pub fn add_download(&mut self, download: Download) {
        self.downloads.push(download);
    }

    pub fn download_count(&self) -> usize {
        self.downloads.len()
    }
}
