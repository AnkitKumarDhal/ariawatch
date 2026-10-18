// Main file, entrypoint of the whole application
mod app;
mod app_state;
mod aria2;
mod domain;
mod logging;
mod monitor;
mod ui;

use crate::app::run;

// Runs the application entry point
fn main() {
    if let Err(error) = run() {
        logging::error(&format!("Application failed: {error}"));
        eprintln!("ariawatch: {error}");
        std::process::exit(1);
    }
}
