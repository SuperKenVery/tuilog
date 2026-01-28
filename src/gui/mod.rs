#[cfg(not(feature = "gpui_gui"))]
pub mod app;
#[cfg(not(feature = "gpui_gui"))]
mod components;
#[cfg(not(feature = "gpui_gui"))]
mod state;
#[cfg(not(feature = "gpui_gui"))]
mod style;

#[cfg(feature = "gpui_gui")]
mod gpui_app;
#[cfg(feature = "gpui_gui")]
mod gpui_state;
#[cfg(feature = "gpui_gui")]
mod gpui_text_input;

use anyhow::Result;
use std::path::PathBuf;

#[cfg(not(feature = "gpui_gui"))]
use dioxus::desktop::{Config, WindowBuilder};
#[cfg(not(feature = "gpui_gui"))]
use dioxus::prelude::*;
#[cfg(not(feature = "gpui_gui"))]
use app::GuiApp;

static INIT_FILE: std::sync::OnceLock<Option<PathBuf>> = std::sync::OnceLock::new();
static INIT_PORT: std::sync::OnceLock<Option<u16>> = std::sync::OnceLock::new();

pub fn run_with_args(file: Option<PathBuf>, port: Option<u16>) -> Result<()> {
    INIT_FILE.set(file.clone()).ok();
    INIT_PORT.set(port).ok();
    #[cfg(feature = "gpui_gui")]
    {
        return gpui_app::run_gpui(file, port);
    }
    #[cfg(not(feature = "gpui_gui"))]
    {
        let window = WindowBuilder::new().with_always_on_top(false);
        let config = Config::default().with_window(window);
        LaunchBuilder::desktop().with_cfg(config).launch(app_with_args);
        Ok(())
    }
}

#[cfg(not(feature = "gpui_gui"))]
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
