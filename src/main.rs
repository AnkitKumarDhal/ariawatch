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

    let endpoint = std::env::var("ARIA2_RPC_URL")
        .unwrap_or_else(|_| String::from("http://127.0.0.1:6800/jsonrpc"));
    let rpc_secret = std::env::var("ARIA2_RPC_SECRET").ok();
    let request_json = match aria2::build_get_version_request(rpc_secret.as_deref()) {
        Ok(json) => json,
        Err(error) => {
            eprintln!("Could not build RPC request: {error}");
            return;
        }
    };

    println!("Calling with aria2.getVersion at {endpoint}");

    let response_json = match aria2::send_rpc_request(&endpoint, &request_json) {
        Ok(body) => body,
        Err(error) => {
            eprintln!("{error}");
            return;
        }
    };

    let rpc_response = match aria2::parse_rpc_response(&response_json) {
        Ok(response) => response,
        Err(error) => {
            eprintln!("Could not pass rpc request: {error}");
            return;
        }
    };

    let rpc_result = match rpc_response.into_result("ariawatch-1") {
        Ok(result) => result,
        Err(error) => {
            eprintln!("RPC Request failed: {error}");
            return;
        }
    };

    println!("Aria2 getVersion result: {rpc_result:#}");

    let active_request_json = match aria2::build_tell_active_request(rpc_secret.as_deref()) {
        Ok(json) => json,
        Err(error) => {
            eprintln!("Could not fetch tellActive request: {error}");
            return;
        }
    };

    let active_response_json = match aria2::send_rpc_request(&endpoint, &active_request_json) {
        Ok(body) => body,
        Err(error) => {
            eprintln!("Could not fetch active downloads: {error}");
            return;
        }
    };

    let active_rpc_json = match aria2::parse_rpc_response(&active_response_json) {
        Ok(response) => response,
        Err(error) => {
            eprintln!("Could not parse tellActive response: {error}");
            return;
        }
    };

    let active_rpc_result = match active_rpc_json.into_result("ariawatch-1") {
        Ok(result) => result,
        Err(error) => {
            eprintln!("tellActive failed: {error}");
            return;
        }
    };

    let active_response = match aria2::parse_download_list(active_rpc_result) {
        Ok(downloads) => downloads,
        Err(error) => {
            eprintln!("Could not decode active downloads: {error}");
            return;
        }
    };

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

    let _download1 = match response.into_download() {
        Ok(download) => download,
        Err(error) => {
            eprintln!("Could not convert aria2 response: {error}");
            return;
        }
    };

    let _download2 = Download::new(
        String::from("somehthing"),
        String::from("Sekiro: Shadows Die Twice"),
        total_size,
        downloaded_size,
        download_speed,
        status,
    );

    let mut state = AppState::new();

    for response in active_response {
        let download = match response.into_download() {
            Ok(download) => download,
            Err(error) => {
                eprintln!("Skipping invalid download: {error}");
                return;
            }
        };

        if let Err(error) = state.add_download(download) {
            eprintln!("Could not track download: {error}");
        }
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
