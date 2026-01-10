## Project Overview

**logviewer** is a terminal-based (TUI) and GUI log viewer written in Rust. It provides interactive filtering, highlighting, and real-time log viewing with support for files, stdin, and TCP network input.

## Build & Test Commands

```bash
# Build TUI only
cargo build --release --no-default-features

# Build with GUI (default)
cargo build --release --features gui

# Run tests
cargo test

# Run TUI mode
cargo run -- [file] [-l port]

# Run GUI mode
cargo run --features gui -- --gui [file] [-l port]
```

## Module Structure

```
src/
├── main.rs              # Entry point, CLI parsing
├── app.rs               # TUI application state and logic
├── state.rs             # Persistent state (.logviewer-state)
├── filter.rs            # Filter expression parser (&&, ||, !)
├── highlight.rs         # Syntax highlighting rules
├── input.rs             # TextInput widget
├── source.rs            # Log sources (file, stdin, network)
├── netinfo.rs           # Network interface discovery
├── ui.rs / tui/mod.rs   # TUI rendering (ratatui)
├── constants.rs         # UI constants
├── gui/
│   ├── mod.rs           # GUI entry point
│   └── app.rs           # Dioxus GUI implementation
└── core/
    ├── filter_state.rs  # FilterState (hide_regex, filter_expr, highlight_expr)
    ├── input_state.rs   # InputMode, InputFields
    ├── log_state.rs     # LogLine, LogState
    └── listen_state.rs  # Network listen state
```

## Key Types

| Type | File | Purpose |
|------|------|---------|
| `App` | `app.rs` | Main TUI application state |
| `GuiAppState` | `gui/app.rs` | GUI application state |
| `AppState` | `state.rs` | Persistent settings (JSON) |
| `FilterState` | `core/filter_state.rs` | Runtime filter/highlight state |
| `FilterExpr` | `filter.rs` | Parsed filter expression AST |
| `LogLine` | `core/log_state.rs` | Single log entry |
| `LogSource` | `source.rs` | Input source enum |
| `TextInput` | `input.rs` | Text input with cursor |

## Architecture Notes

### TUI vs GUI Mode

- **TUI**: Uses `ratatui` + `crossterm`, runs in terminal
- **GUI**: Uses `dioxus` (feature-gated), native desktop window

Both modes share:
- `AppState` for persistence
- `FilterState` for runtime filter state
- `FilterExpr` for filter parsing
- `LogSource` for input handling

### State Persistence

Settings are saved to `.logviewer-state` (JSON) in the working directory:
- `hide_input`: Regex pattern to hide content
- `filter_input`: Filter expression
- `highlight_input`: Highlight expression
- `wrap_lines`: Line wrapping toggle

### Filter Expression Syntax

Parsed by `parse_filter()` in `filter.rs`:
- Simple patterns: `error`, `"quoted string"`
- AND: `error && warning`
- OR: `error || warning`
- NOT: `!debug`
- Grouping: `(error || warning) && !debug`

### Initialization Pattern

When loading saved state on startup:
1. Load text values from `AppState::load()`
2. Parse and apply filters only if text is non-empty (use `trim().is_empty()`)
3. TUI calls `apply_hide()`, `apply_filter()`, `apply_highlight()`
4. GUI parses inline in `GuiAppState::new()`

**Important**: Always check `!text.trim().is_empty()` before parsing to avoid issues with empty/whitespace strings.

## Common Tasks

### Adding a New Filter Type

1. Add field to `FilterState` in `core/filter_state.rs`
2. Add text field to `AppState` in `state.rs`
3. Add UI input in `ui.rs` (TUI) and `gui/app.rs` (GUI)
4. Add `apply_*` method in `app.rs` (TUI) and `gui/app.rs` (GUI)
5. Initialize from saved state in `App::new()` and `GuiAppState::new()`

### Adding a New Log Source

1. Add variant to `LogSource` enum in `source.rs`
2. Handle in `start_source()` function
3. Update CLI args in `main.rs`

### Modifying Highlight Rules

Edit `DEFAULT_RULES` in `highlight.rs`. Rules are applied in order; first match wins for each position.

## Dependencies

- `ratatui` / `crossterm`: TUI framework
- `dioxus`: GUI framework (optional)
- `fancy-regex`: Regex with lookahead/lookbehind
- `notify`: File watching
- `clap`: CLI parsing
- `serde` / `serde_json`: State serialization

## Platform-Specific Code

- `netinfo.rs`: Uses `nix` crate on Unix, `windows` crate on Windows
- Network interface discovery differs by platform
