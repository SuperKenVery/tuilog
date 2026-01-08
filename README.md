# Log Viewer

View and stream your logs in terminal.

Screenshot: viewing `Cargo.lock` in this app
![](./readme_assets/screenshot.png)

## Features

- Simple yet powerful **filter system**. Use syntax like `(kw1 && !kw2) || kw3`.
- **Hide part of any log line** with regular expression. Stop spending your attention on time stamp.
- **Highlight** part of your logs.
- **Network mode**. Works like nc, but with interactive filtering!

## Installation

Coming soon. For now, please run from source.

## Usage


```bash
# View local file and watch for new content
cargo run -- file.log 

# Listen at port and stream logs from network.
# It's just TCP connection, so you can `nc` on the other end.
cargo run -- --listen 8080

```
