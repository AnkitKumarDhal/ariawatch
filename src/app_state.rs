// Manages download state for the application
use crate::domain::download::Download;

// In-memory collection of downloads, tracked by AriaWatch
pub struct AppState {
    downloads: Vec<Download>,
    selected_gid: Option<String>,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            downloads: Vec::new(),
            selected_gid: None,
        }
    }

    fn selected_gid(&self) -> Option<&str> {
        self.selected_gid.as_deref()
    }

    pub fn selected_download(&self) -> Option<&Download> {
        self.downloads
            .iter()
            .find(|download| Some(download.gid.as_str()) == self.selected_gid())
    }

    pub fn select_next(&mut self) {
        if self.downloads.is_empty() {
            self.selected_gid = None;
            return;
        }

        let current_index = self
            .downloads
            .iter()
            .position(|download| Some(download.gid.as_str()) == self.selected_gid());

        let next_index = current_index
            .map(|index| (index + 1) % self.downloads.len())
            .unwrap_or(0);

        self.selected_gid = Some(self.downloads[next_index].gid.clone())
    }

    pub fn select_previous(&mut self) {
        if self.downloads.is_empty() {
            self.selected_gid = None;
            return;
        }

        let current_index = self
            .downloads
            .iter()
            .position(|download| Some(download.gid.as_str()) == self.selected_gid());

        let previous_index = current_index
            .map(|index| {
                if index == 0 {
                    self.downloads.len() - 1
                } else {
                    index - 1
                }
            })
            .unwrap_or(0);

        self.selected_gid = Some(self.downloads[previous_index].gid.clone())
    }

    // Reject a download if its GID is already tracked
    fn ensure_unique_gid(&self, gid: &str) -> Result<(), String> {
        for existing in &self.downloads {
            if existing.gid == gid {
                return Err(format!("Download with GID {} already exists", gid));
            }
        }
        Ok(())
    }

    pub fn add_download(&mut self, download: Download) -> Result<(), String> {
        self.ensure_unique_gid(&download.gid)?;

        if self.selected_gid.is_none() {
            self.selected_gid = Some(download.gid.clone());
        }

        self.downloads.push(download);
        Ok(())
    }

    pub fn remove_download(&mut self, gid: &str) -> Result<(), String> {
        let index = self
            .downloads
            .iter()
            .position(|download| download.gid.as_str() == gid)
            .ok_or_else(|| format!("Download {gid} is not tracked"))?;

        let was_selected = self.selected_gid.as_deref() == Some(gid);

        self.downloads.remove(index);

        if self.downloads.is_empty() {
            self.selected_gid = None;
        } else if was_selected {
            let next_index = index.min(self.downloads.len() - 1);
            self.selected_gid = Some(self.downloads[next_index].gid.clone());
        } else if self.selected_gid.is_none() {
            self.selected_gid = Some(self.downloads[0].gid.clone());
        }

        Ok(())
    }

    // Updates existing download or tracks a new one
    pub fn upsert_download(&mut self, download: Download) {
        let gid = download.gid.clone();

        if let Some(existing) = self
            .downloads
            .iter_mut()
            .find(|existing| existing.gid == download.gid)
        {
            *existing = download;
        } else {
            self.downloads.push(download);
        }

        if self.selected_gid.is_none() {
            self.selected_gid = Some(gid)
        }
    }

    // Replace the tracked list with a fresh validated snapshot
    pub fn replace_downloads(&mut self, downloads: Vec<Download>) -> Result<(), String> {
        let selected_gid = self.selected_gid.clone();
        let mut replacement = Self::new();

        for download in downloads {
            replacement.add_download(download)?;
        }

        if let Some(gid) = selected_gid {
            if replacement
                .downloads
                .iter()
                .any(|download| download.gid == gid)
            {
                replacement.selected_gid = Some(gid)
            }
        }

        self.downloads = replacement.downloads;
        self.selected_gid = replacement.selected_gid;

        Ok(())
    }

    pub fn download_count(&self) -> usize {
        self.downloads.len()
    }

    pub fn active_download_count(&self) -> usize {
        self.downloads
            .iter()
            .filter(|download| download.status.is_active())
            .count()
    }

    pub fn active_gids(&self) -> Vec<String> {
        self.downloads
            .iter()
            .filter(|download| download.status.is_active())
            .map(|download| download.gid.clone())
            .collect()
    }

    // Provides read-only access to the tracked downloads
    pub fn downloads(&self) -> &[Download] {
        &self.downloads
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::status::Status;

    fn make_download(gid: &str, name: &str) -> Download {
        Download::new(
            gid.to_owned(),
            name.to_owned(),
            1000,
            0,
            0,
            Status::Waiting,
            None,
            None,
            None,
        )
    }

    #[test]
    fn first_download_becomes_selected() {
        let mut state = AppState::new();

        state.add_download(make_download("a", "first.zip")).unwrap();

        assert_eq!(state.selected_gid(), Some("a"));
    }

    #[test]
    fn selection_moves_and_wraps() {
        let mut state = AppState::new();

        state.add_download(make_download("a", "first.zip")).unwrap();
        state
            .add_download(make_download("b", "second.zip"))
            .unwrap();

        state.select_next();
        assert_eq!(state.selected_gid(), Some("b"));

        state.select_next();
        assert_eq!(state.selected_gid(), Some("a"));

        state.select_previous();
        assert_eq!(state.selected_gid(), Some("b"));
    }

    #[test]
    fn snapshot_preserves_selected_gid() {
        let mut state = AppState::new();

        state.add_download(make_download("a", "first.zip")).unwrap();
        state
            .add_download(make_download("b", "second.zip"))
            .unwrap();
        state.select_next();

        state
            .replace_downloads(vec![
                make_download("a", "first.zip"),
                make_download("b", "second-updated.zip"),
            ])
            .unwrap();

        assert_eq!(state.selected_gid(), Some("b"));
        assert_eq!(
            state.selected_download().unwrap().name,
            "second-updated.zip"
        );
    }

    #[test]
    fn empty_snapshot_clears_selection() {
        let mut state = AppState::new();

        state.add_download(make_download("a", "first.zip")).unwrap();
        state.replace_downloads(vec![]).unwrap();

        assert_eq!(state.selected_gid(), None);
        assert!(state.selected_download().is_none());
    }

    #[test]
    fn removing_selected_download_selects_another_download() {
        let mut state = AppState::new();

        state.add_download(make_download("a", "first.zip")).unwrap();
        state
            .add_download(make_download("b", "second.zip"))
            .unwrap();

        state.remove_download("a").unwrap();

        assert_eq!(state.selected_gid(), Some("b"));
        assert_eq!(state.download_count(), 1);
    }

    #[test]
    fn removing_last_download_clears_selection() {
        let mut state = AppState::new();
        state.add_download(make_download("a", "first.zip")).unwrap();

        state.remove_download("a").unwrap();

        assert_eq!(state.selected_gid(), None);
        assert_eq!(state.download_count(), 0);
    }
}
