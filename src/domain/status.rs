#[derive(Debug, PartialEq)]
pub enum Status {
    Active,
    Waiting,
    Paused,
    Complete,
    Error,
    Removed,
}
