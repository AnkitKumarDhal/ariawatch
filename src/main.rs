mod app_state;
mod aria2;
mod domain;

use app_state::AppState;
use domain::download::Download;
use domain::status::Status;

fn change_name(name: &mut String) {
    name.push_str(" - watching")
}

fn main() {
    let mut name = String::from("ariawatch");
    change_name(&mut name);

    println!("\n{name}");

    let total_size = match aria2::parse_u64_field("totalLength", "38000000000") {
        Ok(value) => value,
        Err(error) => {
            eprintln!("Could not parse download size: {error}");
            return;
        }
    };

    let downloaded_size = match aria2::parse_u64_field("completedLength", "14000000000") {
        Ok(value) => value,
        Err(error) => {
            eprintln!("Could not parse downloaded size: {error}");
            return;
        }
    };

    let download_speed = match aria2::parse_u64_field("downloadSpeed", "7000000") {
        Ok(value) => value,
        Err(error) => {
            eprintln!("Could not parse download speed: {error}");
            return;
        }
    };

    let status = match Status::from_aria2("active") {
        Ok(status) => status,
        Err(error) => {
            eprintln!("Invalid mock status: {error}");
            return;
        }
    };

    let download1 = Download::new(
        String::from("abc123"),
        String::from("CONTROL: Resonant"),
        total_size,
        downloaded_size,
        download_speed,
        status.clone(),
    );

    let download2 = Download::new(
        String::from("somehthing"),
        String::from("Sekiro: Shadows Die Twice"),
        total_size,
        downloaded_size,
        download_speed,
        status,
    );

    let mut state = AppState::new();

    println!("{}", download2.summary());

    match state.add_download(download1) {
        Ok(()) => println!("Added first download"),
        Err(error) => eprintln!("Could not add first download {error}"),
    }
    match state.add_download(download2) {
        Ok(()) => println!("Added second download"),
        Err(error) => eprintln!("Could not add second download {error}"),
    }

    println!("Tracked downloads: {}", state.download_count());

    println!(
        "\n{:32} {:<14} {:>10} {:>14}",
        "Name", "Status", "Progress", "Remaining"
    );
    println!("{}", "-".repeat(75));

    for download in state.downloads() {
        let progress = match download.percentage_downloaded() {
            Some(percent) => format!("{percent:.1}%"),
            None => String::from("N/A"),
        };

        println!(
            "{:<32} {:<14} {:>10} {:>10.2} GiB",
            download.name,
            download.status.label(),
            progress,
            download.remaining_bytes() as f64 / 1024.0 / 1024.0 / 1024.0
        )
    }
}
