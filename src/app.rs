// Actual starter and init of the application
use crate::{
    app_state::AppState,
    backend,
    config::{RuntimeConfig, RuntimeMode},
    controller, logging,
    paths::AppPaths,
    ui,
};

// Connectst o aria2 and starts the monitor and TUI
// FINALLY its smaller now lol
pub fn run(config: RuntimeConfig) -> Result<(), String> {
    match logging::init() {
        Ok(path) => {
            logging::info(&format!("Ariawatch starting; log file: {}", path.display()));
        }
        Err(error) => {
            eprintln!("ariawatch: file logging unavailable: {error}");
        }
    }

    println!("ariawatch");

    let managed_mode = matches!(&config.mode, RuntimeMode::Managed);

    let backend = backend::create_backend(config)?;
    let version = backend.get_version()?;

    println!("Connected to aria2 version: {version}");
    logging::info(&format!("Connected to aria2 version: {version}"));

    let snapshot = backend.fetch_snapshot()?;

    println!(
        "Fetched downloads: {} active, {} waiting, {} stopped",
        snapshot.active_count, snapshot.waiting_count, snapshot.stopped_count
    );

    logging::info(&format!(
        "Initial downloads: {} active, {} waiting, {} stopped",
        snapshot.active_count, snapshot.waiting_count, snapshot.stopped_count
    ));

    let mut state = AppState::new();

    for download in snapshot.downloads {
        if let Err(error) = state.add_download(download) {
            logging::warn(&format!("Could not track download: {error}"));
        }
    }

    println!("Tracked downloads: {}", state.download_count());

    let updates = backend.start_monitor(state.active_gids())?;
    let exit_choice = ui::run(state, updates, managed_mode, backend.as_ref())?;
    if exit_choice == ui::ExitChoice::StopController {
        let paths = AppPaths::discover()?;
        controller::stop(&paths)?;
    }

    Ok(())
}
