mod domain;
use domain::download::Download;
use domain::status::Status;

fn main() {
    let download = Download {
        gid: String::from("abc123"),
        name: String::from("something"),
        size: 4_000_000_000,
        downloaded: 2_000_000_000,
        speed: 8_000_000,
        status: Status::Active,
    };

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
