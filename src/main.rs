mod app_state;
mod aria2;
mod domain;

use app_state::AppState;

fn main() {
    println!("ariawatch");

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

    let version = rpc_result
        .get("version")
        .and_then(|value| value.as_str())
        .unwrap_or("unknown");

    println!("Connected to aria2 version: {version}");

    let active_downloads = match aria2::build_tell_active_request(rpc_secret.as_deref())
        .and_then(|request| aria2::fetch_downloads(&endpoint, &request))
    {
        Ok(downloads) => downloads,
        Err(error) => {
            eprintln!("Could not fetch active downloads: {error}");
            return;
        }
    };

    let waiting_downloads = match aria2::build_tell_waiting_request(rpc_secret.as_deref(), 0, 1000)
        .and_then(|request| aria2::fetch_downloads(&endpoint, &request))
    {
        Ok(downloads) => downloads,
        Err(error) => {
            eprintln!("Could not fetch waiting downloads: {error}");
            return;
        }
    };

    let stopped_downloads = match aria2::build_tell_stopped_request(rpc_secret.as_deref(), 0, 1000)
        .and_then(|request| aria2::fetch_downloads(&endpoint, &request))
    {
        Ok(downloads) => downloads,
        Err(error) => {
            eprintln!("Could not fetch stopped downloads: {error}");
            return;
        }
    };

    println!(
        "Fetched downloads: {} active, {} waiting, {} stopped",
        active_downloads.len(),
        waiting_downloads.len(),
        stopped_downloads.len(),
    );

    let mut all_downloads = active_downloads;
    all_downloads.extend(waiting_downloads);
    all_downloads.extend(stopped_downloads);

    let mut state = AppState::new();

    for response in all_downloads {
        let download = match response.into_download() {
            Ok(download) => download,
            Err(error) => {
                eprintln!("Skipping invalid download: {error}");
                continue;
            }
        };

        if let Err(error) = state.add_download(download) {
            eprintln!("Could not track download: {error}");
        }
    }

    println!("Tracked downloads: {}", state.download_count());

    println!(
        "\n{:<32} {:<14} {:>10} {:>14} {:>14}",
        "Name", "Status", "Progress", "Speed (B/s)", "Remaining"
    );
    println!("{}", "-".repeat(88));

    for download in state.downloads() {
        let progress = match download.percentage_downloaded() {
            Some(percent) => format!("{percent:.1}%"),
            None => String::from("N/A"),
        };

        println!(
            "{:<32} {:<14} {:>10} {:>14} {:>10.2} GiB",
            download.name,
            download.status.label(),
            progress,
            download.speed,
            download.remaining_bytes() as f64 / 1024.0 / 1024.0 / 1024.0
        );
    }
}
