# Log Viewer

View and stream your logs in TUI or GUI.

Screenshot: viewing `Cargo.lock` in this app
![](./readme_assets/screenshot.png)

## Features

- Simple yet powerful **filter system**. Use syntax like `(kw1 && !kw2) || kw3`, kw can be regular expressions.
- **Hide part of any log line** with regular expression. Stop spending your attention on time stamp.
- **Highlight** part of your logs.
- **Listen on port**. Works like nc, but with interactive filtering!
- **Line start matcher**. Deal with multiline logs with ease.

## Installation

```bash
cargo binstall logviewer
```

## Usage


```bash
# View local file and watch for new content
cargo run -- file.log 

# Listen at port and stream logs from network.
# It's just TCP connection, so you can `nc` on the other end.
cargo run -- --listen 8080

```
