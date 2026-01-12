use anyhow::Result;
use notify::{Event, RecommendedWatcher, RecursiveMode, Watcher};
use std::fs::File;
use std::io::{BufRead, BufReader, Seek, SeekFrom};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;
use std::time::Duration;

pub enum LogSource {
    File(PathBuf),
    Stdin,
    Network(u16),
}

pub enum SourceEvent {
    Line(String),
    Error(String),
    Connected(String),
    Disconnected(String),
}

pub fn start_source(source: LogSource, tx: Sender<SourceEvent>) -> Result<()> {
    match source {
        LogSource::File(path) => start_file_source(path, tx),
        LogSource::Stdin => start_stdin_source(tx),
        LogSource::Network(port) => start_network_source(port, tx),
    }
}

fn start_file_source(path: PathBuf, tx: Sender<SourceEvent>) -> Result<()> {
    let path_clone = path.clone();
    thread::spawn(move || {
        if let Err(e) = run_file_source(path_clone, tx.clone()) {
            let _ = tx.send(SourceEvent::Error(e.to_string()));
        }
    });
    Ok(())
}

fn run_file_source(path: PathBuf, tx: Sender<SourceEvent>) -> Result<()> {
    let mut file = File::open(&path)?;
    let mut reader = BufReader::new(&file);
    let mut line = String::new();

    while reader.read_line(&mut line)? > 0 {
        let trimmed = line.trim_end_matches(['\n', '\r']).to_string();
        if tx.send(SourceEvent::Line(trimmed)).is_err() {
            return Ok(());
        }
        line.clear();
    }

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
                    let trimmed = line.trim_end_matches(['\n', '\r']).to_string();
                    if tx.send(SourceEvent::Line(trimmed)).is_err() {
                        return Ok(());
                    }
                    line.clear();
                }
                pos = file.seek(SeekFrom::Current(0))?;
            }
            Ok(Err(e)) => {
                let _ = tx.send(SourceEvent::Error(e.to_string()));
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => return Ok(()),
        }
    }
}

fn start_stdin_source(tx: Sender<SourceEvent>) -> Result<()> {
    thread::spawn(move || {
        let stdin = std::io::stdin();
        let reader = BufReader::new(stdin.lock());
        for line in reader.lines() {
            match line {
                Ok(l) => {
                    if tx.send(SourceEvent::Line(l)).is_err() {
                        break;
                    }
                }
                Err(e) => {
                    let _ = tx.send(SourceEvent::Error(e.to_string()));
                    break;
                }
            }
        }
    });
    Ok(())
}

fn start_network_source(port: u16, tx: Sender<SourceEvent>) -> Result<()> {
    let listener = TcpListener::bind(format!("[::]:{}", port))
        .or_else(|_| TcpListener::bind(format!("0.0.0.0:{}", port)))?;
    thread::spawn(move || {
        for stream in listener.incoming() {
            match stream {
                Ok(s) => {
                    let tx_clone = tx.clone();
                    thread::spawn(move || handle_client(s, tx_clone));
                }
                Err(e) => {
                    let _ = tx.send(SourceEvent::Error(format!("Accept error: {}", e)));
                }
            }
        }
    });
    Ok(())
}

fn handle_client(stream: TcpStream, tx: Sender<SourceEvent>) {
    let peer = stream
        .peer_addr()
        .map(|a| a.to_string())
        .unwrap_or_else(|_| "unknown".to_string());
    let _ = tx.send(SourceEvent::Connected(peer.clone()));
    let _ = tx.send(SourceEvent::Line(format!("[connected: {}]", peer)));

    let reader = BufReader::new(&stream);
    for line in reader.lines() {
        match line {
            Ok(l) => {
                if tx.send(SourceEvent::Line(l)).is_err() {
                    break;
                }
            }
            Err(e) => {
                let _ = tx.send(SourceEvent::Error(format!("Read error from {}: {}", peer, e)));
                break;
            }
        }
    }
    let _ = tx.send(SourceEvent::Line(format!("[disconnected: {}]", peer)));
    let _ = tx.send(SourceEvent::Disconnected(peer));
}
