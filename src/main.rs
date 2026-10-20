// Main file, entrypoint of the whole application
mod app;
mod app_state;
mod aria2;
mod cli;
mod domain;
mod logging;
mod monitor;
mod ui;

use crate::{app::run, cli::Cli};
use clap::Parser;

// Runs the application entry point
fn main() {
    let cli = Cli::parse();

    if let Err(error) = run(cli) {
        logging::error(&format!("Application failed: {error}"));
        eprintln!("ariawatch: {error}");
        std::process::exit(1);
    }
}
