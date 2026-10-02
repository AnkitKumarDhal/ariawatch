use crate::domain::status::Status;

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
    pub fn summary(&self) -> String {
        format!("{} - {} bytes/s", self.name, self.speed)
    }

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
}

#[cfg(test)]
mod tests {
    use crate::domain::download::Download;
    use crate::domain::status::Status;

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
}
