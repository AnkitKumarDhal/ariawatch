use crate::domain::status::Status;

// Normalized information about one download
#[derive(Debug, PartialEq)]
pub struct Download {
    pub gid: String,
    pub name: String,
    pub size: u64,
    pub downloaded: u64,
    pub speed: u64,
    pub status: Status,
}

impl Download {
    pub fn new(
        gid: String,
        name: String,
        size: u64,
        downloaded: u64,
        speed: u64,
        status: Status,
    ) -> Self {
        Self {
            gid,
            name,
            size,
            downloaded,
            speed,
            status,
        }
    }

    // Calculates remaining bytes, limiting inconsistent values to 0
    pub fn remaining_bytes(&self) -> u64 {
        self.size.saturating_sub(self.downloaded)
    }

    // Calculates percentaed downloaded; returns none if total size is 0
    pub fn percentage_downloaded(&self) -> Option<f64> {
        if self.size == 0 {
            return None;
        }
        let percent = (self.downloaded as f64 / self.size as f64) * 100.0;
        Some(percent.min(100.0))
    }
}

#[cfg(test)]
mod tests {
    use crate::domain::download::Download;
    use crate::domain::status::Status;

    fn make_download(size: u64, downloaded: u64) -> Download {
        Download::new(
            String::from("test123"),
            String::from("test-file.zip"),
            size,
            downloaded,
            0,
            Status::Active,
        )
    }

    #[test]
    fn constructor_testing() {
        let actual = Download::new(
            String::from("xyz098"),
            String::from("Anything"),
            14_000_000_000,
            4_000_000_000,
            8_000_000,
            Status::Waiting,
        );

        let expected = Download {
            gid: String::from("xyz098"),
            name: String::from("Anything"),
            size: 14_000_000_000,
            downloaded: 4_000_000_000,
            speed: 8_000_000,
            status: Status::Waiting,
        };

        assert_eq!(actual, expected);
    }

    #[test]
    fn remaining_bytes_testing() {
        let download = Download::new(
            String::from("xyz098"),
            String::from("Anything"),
            14_000_000_000,
            4_000_000_000,
            8_000_000,
            Status::Waiting,
        );

        let actual = download.remaining_bytes();
        let expected = 10_000_000_000;

        assert_eq!(actual, expected);
    }

    #[test]
    fn percentage_downloaded_testing() {
        let download = Download::new(
            String::from("xyz098"),
            String::from("Anything"),
            14_000_000_000,
            4_000_000_000,
            8_000_000,
            Status::Waiting,
        );
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
}
