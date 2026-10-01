mod domain;
use domain::download::Download;
use domain::status::Status;

fn main() {
    let download = Download::new(
        String::from("abc123"),
        String::from("CONTROL: Resonant"),
        38_000_000_000,
        14_000_000_000,
        7_000_000,
        Status::Active,
    );

    let stat = match download.status {
        Status::Active => "Downloading",
        Status::Error => "Download Error",
        Status::Paused => "Download Paused",
        Status::Waiting => "Waiting for Download",
        Status::Removed => "Download Removed",
        Status::Complete => "Download Complete",
    };

    println!("{} -> {}", stat, download.summary());
}
