// Main file, entrypoint of the whole application
mod app;
mod app_state;
mod aria2;
mod backend;
mod cli;
mod config;
mod controller;
mod domain;
mod logging;
mod monitor;
mod paths;
mod ui;

use crate::{app::run, cli::Cli, config::RuntimeConfig};
use clap::Parser;

// Runs the application entry point
fn main() {
    let cli = Cli::parse();

    if cli.controller {
        if let Err(error) = controller::run_internal() {
            eprintln!("ariawatch: {error}");
            std::process::exit(1);
        }

        return;
    }

    let config = RuntimeConfig::from_cli(cli);

    if let Err(error) = run(config) {
        logging::error(&format!("Application failed: {error}"));
        eprintln!("ariawatch: {error}");
        std::process::exit(1);
    }
}
