use crate::{app_state::AppState, aria2};

// Runs the main application workflow
pub fn run() -> Result<(), String> {
    println!("ariawatch");

    // Read configs from ENv vars
    let endpoint = std::env::var("ARIA2_RPC_URL")
        .unwrap_or_else(|_| String::from("http://127.0.0.1:6800/jsonrpc"));

    let rpc_secret = std::env::var("ARIA2_RPC_SECRET").ok();

    // Build request used to check aria2 conneciton
    let request_json = aria2::build_get_version_request(rpc_secret.as_deref())
        .map_err(|error| format!("Could not build RPC request: {error}"))?;
    println!("Calling aria2.getVersion at {endpoint}");

    // Send request and parse aria2's JSON-RPC response
    let response_json = aria2::send_rpc_request(&endpoint, &request_json)?;
    let rpc_response = aria2::parse_rpc_response(&response_json)?;

    // Validate the reponse and extract its result
    let rpc_result = rpc_response.into_result("ariawatch-1")?;

    // Determine version
    let version = rpc_result
        .get("version")
        .and_then(|value| value.as_str())
        .unwrap_or("unknown");

    println!("Connected to aria2 version: {version}");

    // Fetching and using all downloads list
    let (active_count, waiting_count, stopped_count, all_downloads) =
        fetch_all_downloads(&endpoint, rpc_secret.as_deref())?;

    println!(
        "Fetched downloads: {active_count} active, {waiting_count} waiting, {stopped_count} stopped"
    );

    // Create and use state to track downloads
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
            eprintln!("Could not track download: {error}")
        }
    }

    println!("Tracked downloads: {}", state.download_count());

    // Printing the download table
    print_download_table(&state);

    Ok(())
}

// Fetch all downloads and return 3 type download counts and list of all downloads
fn fetch_all_downloads(
    endpoint: &str,
    rpc_secret: Option<&str>,
) -> Result<(usize, usize, usize, Vec<aria2::Aria2DownloadResponse>), String> {
    // Fetch and build active download list
    let active_request = aria2::build_tell_active_request(rpc_secret)
        .map_err(|error| format!("Could not build active-download request: {error}"))?;
    let active_downloads = aria2::fetch_downloads(endpoint, &active_request)?;

    // Fetch and build waiting download list
    let waiting_request = aria2::build_tell_waiting_request(rpc_secret, 0, 1000)
        .map_err(|error| format!("Could not build waiting-download request: {error}"))?;
    let waiting_downloads = aria2::fetch_downloads(endpoint, &waiting_request)?;

    // Fetch and build stopped download list
    let stopped_request = aria2::build_tell_stopped_request(rpc_secret, 0, 1000)
        .map_err(|error| format!("Could not build stopped-download request: {error}"))?;
    let stopped_downloads = aria2::fetch_downloads(endpoint, &stopped_request)?;

    // Get download counts for all 3 types
    let active_count = active_downloads.len();
    let waiting_count = waiting_downloads.len();
    let stopped_count = stopped_downloads.len();

    // Make all types of downloads into one list
    let mut all_downloads = active_downloads;
    all_downloads.extend(waiting_downloads);
    all_downloads.extend(stopped_downloads);

    Ok((active_count, waiting_count, stopped_count, all_downloads))
}

fn print_download_table(state: &AppState) {
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
