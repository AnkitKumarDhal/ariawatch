mod domain;
use domain::download::Download;

fn main() {
    let download = Download {
        gid: String::from("abc123"),
        name: String::from("something"),
        size: 4_000_000_000,
        downloaded: 2_000_000_000,
        speed: 8_000_000,
    };

    println!("{}: {} bytes/s", download.name, download.speed)
}
