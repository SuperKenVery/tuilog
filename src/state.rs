use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

const STATE_FILE: &str = ".logviewer-state";

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct AppState {
    pub hide_input: String,
    pub filter_input: String,
    pub highlight_input: String,
}

impl AppState {
    pub fn load() -> Self {
        let path = Path::new(STATE_FILE);
        if path.exists() {
            if let Ok(content) = fs::read_to_string(path) {
                if let Ok(state) = serde_json::from_str(&content) {
                    return state;
                }
            }
        }
        Self::default()
    }

    pub fn save(&self) {
        let path = Path::new(STATE_FILE);
        if let Ok(content) = serde_json::to_string_pretty(self) {
            let _ = fs::write(path, content);
        }
    }
}
