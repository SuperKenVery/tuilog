use anyhow::Result;
use fancy_regex::Regex;
use notify::{Event, RecommendedWatcher, RecursiveMode, Watcher};
use std::fs::File;
use std::io::{BufRead, BufReader, Seek, SeekFrom};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

pub enum LogSource {
    File(PathBuf),
    Stdin,
    Network(u16),
}

pub enum SourceEvent {
    Line(String),
    SystemLine(String),
    Error(String),
    Connected(String),
    Disconnected(String),
}

pub fn start_source(source: LogSource, tx: Sender<SourceEvent>, line_start_regex: Option<Arc<Regex>>) -> Result<()> {
    match source {
        LogSource::File(path) => start_file_source(path, tx, line_start_regex),
        LogSource::Stdin => start_stdin_source(tx, line_start_regex),
        LogSource::Network(port) => start_network_source(port, tx, line_start_regex),
    }
}

fn start_file_source(path: PathBuf, tx: Sender<SourceEvent>, line_start_regex: Option<Arc<Regex>>) -> Result<()> {
    let path_clone = path.clone();
    thread::spawn(move || {
        if let Err(e) = run_file_source(path_clone, tx.clone(), line_start_regex) {
            let _ = tx.send(SourceEvent::Error(e.to_string()));
        }
    });
    Ok(())
}

struct MultilineAggregator {
    regex: Option<Arc<Regex>>,
    pending: Option<String>,
}

impl MultilineAggregator {
    fn new(regex: Option<Arc<Regex>>) -> Self {
        Self { regex, pending: None }
    }

    fn process_line(&mut self, line: &str, tx: &Sender<SourceEvent>) -> bool {
        let trimmed = line.trim_end_matches(['\n', '\r']);
        match &self.regex {
            None => {
                if tx.send(SourceEvent::Line(trimmed.to_string())).is_err() {
                    return false;
                }
            }
            Some(re) => {
                let is_start = re.is_match(trimmed).unwrap_or(false);
                if is_start {
                    if let Some(pending) = self.pending.take() {
                        if tx.send(SourceEvent::Line(pending)).is_err() {
                            return false;
                        }
                    }
                    self.pending = Some(trimmed.to_string());
                } else {
                    match &mut self.pending {
                        Some(p) => {
                            p.push('\n');
                            p.push_str(trimmed);
                        }
                        None => {
                            self.pending = Some(trimmed.to_string());
                        }
                    }
                }
            }
        }
        true
    }

    fn flush(&mut self, tx: &Sender<SourceEvent>) {
        if let Some(pending) = self.pending.take() {
            let _ = tx.send(SourceEvent::Line(pending));
        }
    }
}

fn run_file_source(path: PathBuf, tx: Sender<SourceEvent>, line_start_regex: Option<Arc<Regex>>) -> Result<()> {
    let mut file = File::open(&path)?;
    let mut reader = BufReader::new(&file);
    let mut line = String::new();
    let mut aggregator = MultilineAggregator::new(line_start_regex);

    while reader.read_line(&mut line)? > 0 {
        if !aggregator.process_line(&line, &tx) {
            return Ok(());
        }
        line.clear();
    }
    aggregator.flush(&tx);

    let mut pos = file.seek(SeekFrom::Current(0))?;

    let (notify_tx, notify_rx): (Sender<notify::Result<Event>>, Receiver<notify::Result<Event>>) =
        mpsc::channel();
    let mut watcher = RecommendedWatcher::new(
        move |res| {
            let _ = notify_tx.send(res);
        },
        notify::Config::default().with_poll_interval(Duration::from_millis(100)),
    )?;
    watcher.watch(&path, RecursiveMode::NonRecursive)?;

    loop {
        match notify_rx.recv_timeout(Duration::from_millis(500)) {
            Ok(Ok(_)) | Err(mpsc::RecvTimeoutError::Timeout) => {
                file = File::open(&path)?;
                file.seek(SeekFrom::Start(pos))?;
                reader = BufReader::new(&file);

                while reader.read_line(&mut line)? > 0 {
                    if !aggregator.process_line(&line, &tx) {
                        return Ok(());
                    }
                    line.clear();
                }
                pos = file.seek(SeekFrom::Current(0))?;
            }
            Ok(Err(e)) => {
                let _ = tx.send(SourceEvent::Error(e.to_string()));
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                aggregator.flush(&tx);
                return Ok(());
            }
        }
    }
}

fn start_stdin_source(tx: Sender<SourceEvent>, line_start_regex: Option<Arc<Regex>>) -> Result<()> {
    thread::spawn(move || {
        let stdin = std::io::stdin();
        let reader = BufReader::new(stdin.lock());
        let mut aggregator = MultilineAggregator::new(line_start_regex);
        for line in reader.lines() {
            match line {
                Ok(l) => {
                    if !aggregator.process_line(&l, &tx) {
                        break;
                    }
                }
                Err(e) => {
                    let _ = tx.send(SourceEvent::Error(e.to_string()));
                    break;
                }
            }
        }
        aggregator.flush(&tx);
    });
    Ok(())
}

fn start_network_source(port: u16, tx: Sender<SourceEvent>, line_start_regex: Option<Arc<Regex>>) -> Result<()> {
    let listener = TcpListener::bind(format!("[::]:{}", port))
        .or_else(|_| TcpListener::bind(format!("0.0.0.0:{}", port)))?;
    thread::spawn(move || {
        for stream in listener.incoming() {
            match stream {
                Ok(s) => {
                    let tx_clone = tx.clone();
                    let regex_clone = line_start_regex.clone();
                    thread::spawn(move || handle_client(s, tx_clone, regex_clone));
                }
                Err(e) => {
                    let _ = tx.send(SourceEvent::Error(format!("Accept error: {}", e)));
                }
            }
        }
    });
    Ok(())
}

fn handle_client(stream: TcpStream, tx: Sender<SourceEvent>, line_start_regex: Option<Arc<Regex>>) {
    let peer = stream
        .peer_addr()
        .map(|a| a.to_string())
        .unwrap_or_else(|_| "unknown".to_string());
    
    if let Err(e) = stream.set_nodelay(true) {
        let _ = tx.send(SourceEvent::Error(format!("Failed to set TCP_NODELAY: {}", e)));
    }
    
    let keepalive = socket2::TcpKeepalive::new()
        .with_time(Duration::from_secs(10))
        .with_interval(Duration::from_secs(5));
    
    let socket_ref = socket2::SockRef::from(&stream);
    if let Err(e) = socket_ref.set_tcp_keepalive(&keepalive) {
        let _ = tx.send(SourceEvent::Error(format!("Failed to set TCP keepalive: {}", e)));
    }
    
    let _ = tx.send(SourceEvent::Connected(peer.clone()));
    let _ = tx.send(SourceEvent::SystemLine(format!("[connected: {}]", peer)));

    let reader = BufReader::new(&stream);
    let mut aggregator = MultilineAggregator::new(line_start_regex);
    for line in reader.lines() {
        match line {
            Ok(l) => {
                if !aggregator.process_line(&l, &tx) {
                    break;
                }
            }
            Err(e) => {
                let _ = tx.send(SourceEvent::Error(format!("Read error from {}: {}", peer, e)));
                break;
            }
        }
    }
    aggregator.flush(&tx);
    let _ = tx.send(SourceEvent::SystemLine(format!("[disconnected: {}]", peer)));
    let _ = tx.send(SourceEvent::Disconnected(peer));
}
