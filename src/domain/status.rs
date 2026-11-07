// This file, defines aria2 download statusses

// aria2 download states
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    Active,
    Seeding,
    Waiting,
    Paused,
    Complete,
    Error,
    Removed,
}

impl Status {
    // Returns actual status text to be displayed
    pub fn label(&self) -> &'static str {
        match self {
            Self::Active => "Downloading",
            Self::Seeding => "Seeding",
            Self::Waiting => "Queued",
            Self::Paused => "Paused",
            Self::Complete => "Completed",
            Self::Error => "Error",
            Self::Removed => "Removed",
        }
    }

    // Identifies active downloads (so updates can be fetched for them)
    pub fn is_active(&self) -> bool {
        matches!(self, Self::Active | Self::Seeding)
    }

    // Converts aria2 status strings into our enum
    pub fn from_aria2(value: &str) -> Result<Self, String> {
        match value {
            "active" => Ok(Status::Active),
            "waiting" => Ok(Status::Waiting),
            "paused" => Ok(Status::Paused),
            "complete" => Ok(Status::Complete),
            "error" => Ok(Status::Error),
            "removed" => Ok(Status::Removed),
            _ => Err(format!("Unknown aria2 status: {}", value)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Status;

    #[test]
    fn seeding_has_its_own_label() {
        assert_eq!(Status::Seeding.label(), "Seeding");
    }

    #[test]
    fn seeding_is_considered_active() {
        assert!(Status::Seeding.is_active());
    }

    #[test]
    fn ordinary_downloading_status_remains_active() {
        assert_eq!(Status::from_aria2("active").unwrap(), Status::Active);
    }
}
