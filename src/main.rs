mod domain;
use domain::download::Download;
use domain::status::Status;

fn change_name(name: &mut String) {
    name.push_str(" - watching")
}

fn main() {
    let mut name = String::from("ariawatch");
    change_name(&mut name);

    println!("\n{name}");

    let download1 = Download::new(
        String::from("abc123"),
        String::from("CONTROL: Resonant"),
        38_000_000_000,
        14_000_000_000,
        7_000_000,
        Status::Active,
    );

    let download2 = Download::new(
        String::from("abc123"),
        String::from("CONTROL: Resonant"),
        38_000_000_000,
        14_000_000_000,
        7_000_000,
        Status::Active,
    );

    let download3 = &download2;

    let stat = match download2.status {
        Status::Active => "Downloading",
        Status::Error => "Download Error",
        Status::Paused => "Download Paused",
        Status::Waiting => "Waiting for Download",
        Status::Removed => "Download Removed",
        Status::Complete => "Download Complete",
    };

    let status_copy = download1.status.clone();

    println!("{status_copy:?}");

    println!("{} -> {}", stat, download1.summary());
    println!("{download1:?}");
    if download1.status == Status::Active {
        println!("\nIts downloading dawg")
    }

    if download1 == download2 {
        println!("Both are the same downloads nigga")
    }

    println!("{}", download3.summary());
}
