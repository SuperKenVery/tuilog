pub mod app;
mod components;
mod state;
mod style;

use anyhow::Result;
use dioxus::prelude::*;
use std::path::PathBuf;

use app::GuiApp;

static INIT_FILE: std::sync::OnceLock<Option<PathBuf>> = std::sync::OnceLock::new();
static INIT_PORT: std::sync::OnceLock<Option<u16>> = std::sync::OnceLock::new();

pub fn run_with_args(file: Option<PathBuf>, port: Option<u16>) -> Result<()> {
    INIT_FILE.set(file).ok();
    INIT_PORT.set(port).ok();

    // dioxus_native::launch(app_with_args);
    dioxus::launch(app_with_args);
    Ok(())
}

fn app_with_args() -> Element {
    let file = INIT_FILE.get().cloned().flatten();
    let port = INIT_PORT.get().cloned().flatten();

    rsx! {
        GuiApp {
            file: file,
            port: port,
        }
    }
}
