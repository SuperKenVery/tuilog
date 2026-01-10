pub mod app;

use anyhow::Result;
use std::path::PathBuf;

pub fn run_with_args(file: Option<PathBuf>, port: Option<u16>) -> Result<()> {
    app::run_gui(file, port);
    Ok(())
}
