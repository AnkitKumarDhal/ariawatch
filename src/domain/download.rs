// Defines download data and progress calculations
use crate::domain::status::Status;

// information about one file belonging to a download (feels like i'm in an english class lmao)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DownloadFile {
    pub path: String,
    pub size: u64,
    pub downloaded: u64,
}

// Normalized information about one download (updated to be a collection of DownloadFile's)
#[derive(Debug, PartialEq)]
pub struct Download {
    pub gid: String,
    pub name: String,
    pub size: u64,
    pub downloaded: u64,
    pub speed: u64,
    pub status: Status,
    pub files: Vec<DownloadFile>,
    pub directory: Option<String>,
    pub source_url: Option<String>,
    pub error_message: Option<String>,
}

impl Download {
    pub fn remaining_bytes(&self) -> u64 {
        self.size.saturating_sub(self.downloaded)
    }

    pub fn estimated_remaining_seconds(&self) -> Option<u64> {
        if self.speed == 0 || self.size == 0 {
            return None;
        }

        let remaining = self.remaining_bytes();
        let mut seconds = remaining / self.speed;

        if !remaining.is_multiple_of(self.speed) {
            seconds += 1;
        }

        Some(seconds)
    }

    pub fn percentage_downloaded(&self) -> Option<f64> {
        if self.size == 0 {
            return None;
        }
        let percent = (self.downloaded as f64 / self.size as f64) * 100.0;
        Some(percent.min(100.0))
    }
}

// These f**king tests <~3
#[cfg(test)]
mod tests {
    use crate::domain::download::Download;
    use crate::domain::status::Status;

    fn make_download(size: u64, downloaded: u64) -> Download {
        Download {
            gid: String::from("test123"),
            name: String::from("test-file.zip"),
            size,
            downloaded,
            speed: 0,
            status: Status::Active,
            files: Vec::new(),
            directory: None,
            source_url: None,
            error_message: None,
        }
    }

    #[test]
    fn remaining_bytes_testing() {
        let download = make_download(14_000_000_000, 4_000_000_000);
        let actual = download.remaining_bytes();
        let expected = 10_000_000_000;
        assert_eq!(actual, expected);
    }

    #[test]
    fn percentage_downloaded_testing() {
        let download = make_download(14_000_000_000, 4_000_000_000);
        let actual = download.percentage_downloaded();
        let expected = Some(28.57142857142857);
        assert_eq!(actual, expected);
    }

    #[test]
    fn percentage_downloaded_returns_100_when_complete() {
        let download = make_download(100, 100);

        assert_eq!(download.percentage_downloaded(), Some(100.0));
    }

    #[test]
    fn percentage_downloaded_returns_zero_when_nothing_downloaded() {
        let download = make_download(100, 0);

        assert_eq!(download.percentage_downloaded(), Some(0.0));
    }

    #[test]
    fn percentage_downloaded_returns_none_when_size_is_zero() {
        let download = make_download(0, 0);

        assert_eq!(download.percentage_downloaded(), None);
    }

    #[test]
    fn percentage_downloaded_never_exceeds_100() {
        let download = make_download(100, 120);

        assert_eq!(download.percentage_downloaded(), Some(100.0));
    }

    #[test]
    fn estimated_remaining_seconds_rounds_up() {
        let mut download = make_download(1000, 1);
        download.speed = 400;

        assert_eq!(download.estimated_remaining_seconds(), Some(3));
    }

    #[test]
    fn estimated_remaining_seconds_returns_none_when_speed_is_zero() {
        let download = make_download(100, 0);

        assert_eq!(download.estimated_remaining_seconds(), None);
    }
}
