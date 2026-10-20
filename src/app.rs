// Actual starter and init of the application
use crate::{app_state::AppState, aria2, cli::Cli, logging, monitor, ui};

// Connectst o aria2 and starts the monitor and TUI
pub fn run(cli: Cli) -> Result<(), String> {
    match logging::init() {
        Ok(path) => {
            logging::info(&format!("Ariawatch starting; log file: {}", path.display()));
        }
        Err(error) => {
            eprintln!("ariawatch: file logging unavailable: {error}");
        }
    }

    println!("ariawatch");

    if !cli.external {
        return Err("Managed mode is not implemented yet. During development, use `ariawatch --external` to connect to an existing aria2 server".to_owned());
    }

    let endpoint = cli
        .rpc_url
        .or_else(|| std::env::var("ARIA2_RPC_URL").ok())
        .unwrap_or_else(|| String::from("http://127.0.0.1:6800/jsonrpc"));

    let rpc_secret = std::env::var("ARIA2_RPC_SECRET").ok();

    let request_json = aria2::build_get_version_request(rpc_secret.as_deref())
        .map_err(|error| format!("Could not build RPC request: {error}"))?;
    println!("Calling aria2.getVersion at {endpoint}");

    let response_json = aria2::send_rpc_request(&endpoint, &request_json)?;
    let rpc_response = aria2::parse_rpc_response(&response_json)?;
    let rpc_result = rpc_response.into_result("ariawatch-1")?;
    let version = rpc_result
        .get("version")
        .and_then(|value| value.as_str())
        .unwrap_or("unknown");

    println!("Connected to aria2 version: {version}");
    logging::info(&format!("Connected to aria2 version: {version}"));

    let (active_count, waiting_count, stopped_count, all_downloads) =
        aria2::fetch_all_downloads(&endpoint, rpc_secret.as_deref())?;
    println!(
        "Fetched downloads: {active_count} active, {waiting_count} waiting, {stopped_count} stopped"
    );
    logging::info(&format!(
        "Initial snapshots: {active_count} active, {waiting_count} waiting, {stopped_count} stopped"
    ));

    let mut state = AppState::new();

    for response in all_downloads {
        let download = match response.into_download() {
            Ok(download) => download,
            Err(error) => {
                logging::warn(&format!("Skipping invalid download: {error}"));
                continue;
            }
        };

        if let Err(error) = state.add_download(download) {
            logging::warn(&format!("Could not track download: {error}"));
        }
    }

    println!("Tracked downloads: {}", state.download_count());

    let updates = monitor::start(endpoint.clone(), rpc_secret.clone(), state.active_gids())?;
    ui::run(state, updates)?;

    Ok(())
}
