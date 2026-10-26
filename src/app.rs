// Actual starter and init of the application
use crate::{
    app_state::AppState,
    backend,
    config::{RuntimeConfig, RuntimeMode},
    controller,
    history::HistoryStore,
    logging,
    paths::AppPaths,
    ui,
};

fn should_stop_controller(managed_mode: bool, exit_choice: ui::ExitChoice) -> bool {
    managed_mode && exit_choice == ui::ExitChoice::StopController
}

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

    // this is the connection to the database
    let paths = AppPaths::discover()?;
    let history_path = match &config.mode {
        RuntimeMode::Managed => paths.database_file(),
        RuntimeMode::External(external) => paths.external_database_file(&external.rpc_url),
    };
    let mut history = HistoryStore::open(&history_path)?;

    let backend = backend::create_backend(config)?;
    let version = backend.get_version()?;

    println!("Connected to aria2 version: {version}");
    logging::info(&format!("Connected to aria2 version: {version}"));

    let snapshot = backend.fetch_snapshot()?;

    if snapshot.is_complete {
        let unavailable_count = history.reconcile_snapshot(&snapshot.downloads)?;
        if unavailable_count > 0 {
            logging::info(&format!(
                "Marked {unavailable_count} stale history record(s) as unavailable"
            ));
        }
    } else {
        history.record_downloads(&snapshot.downloads)?;
        logging::warn("Initial aria2 snapshot was incomplete; history reconcilation was skipped");
    }

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
    let exit_choice = ui::run(state, updates, managed_mode, backend.as_ref(), &mut history)?;
    if should_stop_controller(managed_mode, exit_choice) {
        controller::stop(&paths)?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::should_stop_controller;
    use crate::ui::ExitChoice;

    #[test]
    fn managed_mode_stops_controller_only_when_requested() {
        assert!(should_stop_controller(true, ExitChoice::StopController));

        assert!(!should_stop_controller(true, ExitChoice::LeaveRunning));
    }

    #[test]
    fn external_mode_never_stops_controller() {
        assert!(!should_stop_controller(false, ExitChoice::StopController));

        assert!(!should_stop_controller(false, ExitChoice::LeaveRunning));
    }
}
