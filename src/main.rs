mod app;
mod app_state;
mod aria2;
mod domain;
mod ui;

use crate::app::run;

fn main() {
    if let Err(error) = run() {
        eprintln!("ariawatch: {error}");
        std::process::exit(1);
    }
}
