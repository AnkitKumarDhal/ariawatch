mod app_state;
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
        38_000_000_000,
        14_000_000_000,
        7_000_000,
        status.clone(),
    );

    let download2 = Download::new(
        String::from("somehthing"),
        String::from("CONTROL: Resonant"),
        38_000_000_000,
        14_000_000_000,
        7_000_000,
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
