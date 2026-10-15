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

    let mock_json = r#"
{
    "gid": "abc123",
    "status": "active",
    "totalLength": "38000000000",
    "completedLength": "14000000000",
    "downloadSpeed": "7000000",
    "files": [
        {
          "path": "/downloads/CONTROL Resonant.mkv"
        }
    ]
}
"#;

    let request_json = match aria2::build_get_version_request() {
        Ok(json) => json,
        Err(error) => {
            eprintln!("Could not build aria2 request: {error}");
            return;
        }
    };

    println!("JSON-RPC request: {request_json}");

    let mock_rpc_response = r#"
{
    "jsonrpc": "2.0",
    "id": "ariawatch-1",
    "result": {
        "version": "1.37.0",
        "enabledFeatures": ["HTTPS", "BitTorrent"]
    }
}
"#;

    let rpc_response = match aria2::parse_rpc_response(mock_rpc_response) {
        Ok(response) => response,
        Err(error) => {
            eprintln!("Could not parse rpc response: {error}");
            return;
        }
    };

    println!("Parse rpc response: {rpc_response:#?}");

    let response = match aria2::parse_download_response(mock_json) {
        Ok(response) => response,
        Err(error) => {
            eprintln!("Could not parse aria2 repsonse: {error}");
            return;
        }
    };

    let total_size = match aria2::parse_u64_field("totalLength", &response.total_length) {
        Ok(value) => value,
        Err(error) => {
            eprintln!("Could not parse download size: {error}");
            return;
        }
    };

    let downloaded_size =
        match aria2::parse_u64_field("completedLength", &response.completed_length) {
            Ok(value) => value,
            Err(error) => {
                eprintln!("Could not parse downloaded size: {error}");
                return;
            }
        };

    let download_speed = match aria2::parse_u64_field("downloadSpeed", &response.download_speed) {
        Ok(value) => value,
        Err(error) => {
            eprintln!("Could not parse download speed: {error}");
            return;
        }
    };

    let status = match Status::from_aria2(&response.status) {
        Ok(status) => status,
        Err(error) => {
            eprintln!("Invalid mock status: {error}");
            return;
        }
    };

    let download1 = match response.into_download() {
        Ok(download) => download,
        Err(error) => {
            eprintln!("Could not convert aria2 response: {error}");
            return;
        }
    };

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
