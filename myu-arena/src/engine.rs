//! Engine process management and communication via myu-protocol.

use std::{
    fs::File,
    io::{BufRead, BufReader, BufWriter, Write},
    os::unix::process::CommandExt,
    path::{Path, PathBuf},
    process::{Child, ChildStdin, Command, Stdio},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, RecvTimeoutError},
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

use crate::protocol::{EngineMsg, UserMsg, limits::Limits, mv::Mv};

/// A communication log entry
#[derive(Debug, Clone)]
pub struct LogEntry {
    pub timestamp_ms: u64,
    pub direction: LogDirection,
    pub content: String,
}

#[derive(Debug, Clone, Copy)]
pub enum LogDirection {
    Sent,
    Received,
}

/// Reason for writing a log file
#[derive(Debug, Clone, Copy)]
pub enum LogReason {
    IllegalMove,
    NoMove,
    Timeout,
    Crash,
    ProtocolError,
}

impl LogReason {
    fn as_str(self) -> &'static str {
        match self {
            Self::IllegalMove => "illegal_move",
            Self::NoMove => "no_move",
            Self::Timeout => "timeout",
            Self::Crash => "crash",
            Self::ProtocolError => "protocol_error",
        }
    }
}

/// Result of waiting for a move from the engine
#[derive(Debug)]
pub enum MoveResult {
    Move(Mv),
    NoMove, // Engine returned None (shouldn't happen in normal play)
    Timeout,
    Crash,
    Stopped, // Graceful shutdown in progress
    #[allow(dead_code)]
    ProtocolError(String),
    #[allow(dead_code)]
    EngineError(String),
}

/// Engine wrapper that handles communication
pub struct Engine {
    name: String,
    path: PathBuf,
    child: Child,
    stdin: BufWriter<ChildStdin>,
    msg_rx: Receiver<Result<(EngineMsg, String), String>>,
    _reader_thread: JoinHandle<()>,
    logs_dir: Option<PathBuf>,
    log: Vec<LogEntry>,
    start_time: Instant,
    engine_name: Option<String>,
    timeout_ms: u64,
    stop_flag: Arc<AtomicBool>,
}

impl Engine {
    /// Spawn a new engine process
    pub fn spawn(
        name: impl Into<String>,
        path: &Path,
        time_ms: u64,
        timeout_leniency: f64,
        logs_dir: Option<PathBuf>,
        stop_flag: Arc<AtomicBool>,
        settings: Vec<serde_json::Map<String, serde_json::Value>>,
    ) -> Result<Self, String> {
        let name = name.into();
        let mut child = Command::new(path)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .process_group(0)
            .spawn()
            .map_err(|e| format!("Failed to spawn {name}: {e}"))?;

        let stdin = child.stdin.take().ok_or("Failed to capture stdin")?;
        let stdout = child.stdout.take().ok_or("Failed to capture stdout")?;

        let timeout_ms = (time_ms as f64 * timeout_leniency) as u64;

        // Create channel for messages from reader thread
        let (msg_tx, msg_rx) = mpsc::channel();

        // Spawn reader thread
        let reader_thread = thread::spawn(move || {
            let mut reader = BufReader::new(stdout);
            let mut line = String::new();
            loop {
                line.clear();
                match reader.read_line(&mut line) {
                    Ok(0) => {
                        // EOF - engine closed stdout
                        _ = msg_tx.send(Err("Engine closed stdout".into()));
                        break;
                    }
                    Ok(_) => {
                        let trimmed = line.trim();
                        if trimmed.is_empty() {
                            continue;
                        }
                        match serde_json::from_str::<EngineMsg>(trimmed) {
                            Ok(msg) => {
                                if msg_tx.send(Ok((msg, trimmed.to_string()))).is_err() {
                                    break;
                                }
                            }
                            Err(e) => {
                                if msg_tx.send(Err(format!("Parse error: {e}, line: {trimmed}"))).is_err() {
                                    break;
                                }
                            }
                        }
                    }
                    Err(e) => {
                        _ = msg_tx.send(Err(format!("Read error: {e}")));
                        break;
                    }
                }
            }
        });

        let mut engine = Self {
            name,
            path: path.to_path_buf(),
            child,
            stdin: BufWriter::new(stdin),
            msg_rx,
            _reader_thread: reader_thread,
            logs_dir,
            log: Vec::new(),
            start_time: Instant::now(),
            engine_name: None,
            timeout_ms,
            stop_flag,
        };

        // Wait for engine identification
        engine.wait_for_about()?;

        // Send settings
        for s in settings {
            engine.set(s)?;
        }

        Ok(engine)
    }

    fn wait_for_about(&mut self) -> Result<(), String> {
        // Engines should send About message immediately on startup
        match self.msg_rx.recv_timeout(Duration::from_secs(5)) {
            Ok(Ok((msg @ EngineMsg::About { .. }, raw))) => {
                if let EngineMsg::About { name } = msg {
                    self.engine_name = name;
                }
                self.log_received(&raw);
                Ok(())
            }
            Ok(Ok((msg, raw))) => {
                self.log_received(&raw);
                Err(format!("Engine {} sent {:?} instead of About", self.name, msg))
            }
            Ok(Err(e)) => Err(e),
            Err(RecvTimeoutError::Timeout) => Err(format!("Engine {} did not send About message (timeout)", self.name)),
            Err(RecvTimeoutError::Disconnected) => Err(format!("Engine {} reader disconnected", self.name)),
        }
    }

    fn log(&mut self, direction: LogDirection, content: &str) {
        self.log.push(LogEntry {
            timestamp_ms: self.start_time.elapsed().as_millis() as u64,
            direction,
            content: content.to_string(),
        });
    }

    fn log_received(&mut self, content: &str) {
        self.log(LogDirection::Received, content);
    }

    /// Write the communication log to the logs directory
    pub fn write_log(&self, reason: LogReason) {
        let Some(ref logs_dir) = self.logs_dir else { return };

        let timestamp =
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);

        let filename = format!("{}_{timestamp}_{}.log", self.name, reason.as_str());
        let path = logs_dir.join(filename);

        if let Ok(mut file) = File::create(&path) {
            writeln!(file, "Engine: {} ({})", self.name, self.path.display()).ok();
            writeln!(file, "Reason: {}", reason.as_str()).ok();
            writeln!(file, "---").ok();
            for entry in &self.log {
                let dir = match entry.direction {
                    LogDirection::Sent => ">>>",
                    LogDirection::Received => "<<<",
                };
                writeln!(file, "[{:>8}ms] {dir} {}", entry.timestamp_ms, entry.content).ok();
            }
        }
    }

    fn send_message(&mut self, msg: &UserMsg) -> Result<(), String> {
        let line = serde_json::to_string(msg).map_err(|e| format!("Serialization error: {e}"))?;
        self.log(LogDirection::Sent, &line);
        writeln!(self.stdin, "{line}").map_err(|e| format!("Write error: {e}"))?;
        self.stdin.flush().map_err(|e| format!("Flush error: {e}"))?;
        Ok(())
    }

    /// Send Set command
    pub fn set(&mut self, settings: serde_json::Map<String, serde_json::Value>) -> Result<(), String> {
        self.send_message(&UserMsg::Set(settings))
    }

    fn recv_message_timeout(&mut self, timeout: Duration) -> Result<Option<EngineMsg>, String> {
        match self.msg_rx.recv_timeout(timeout) {
            Ok(Ok((msg, raw))) => {
                self.log_received(&raw);
                Ok(Some(msg))
            }
            Ok(Err(e)) => Err(e),
            Err(RecvTimeoutError::Timeout) => Ok(None),
            Err(RecvTimeoutError::Disconnected) => Err("Engine reader disconnected".into()),
        }
    }

    /// Send Sync command and wait for Ready
    pub fn sync(&mut self) -> Result<(), String> {
        self.send_message(&UserMsg::Sync)?;
        self.wait_for_ready()
    }

    /// Reset the engine for a new game
    pub fn reset(&mut self) -> Result<(), String> {
        self.send_message(&UserMsg::Reset)?;
        self.send_message(&UserMsg::Sync)?;
        self.wait_for_ready()?;
        // Clear the log for the new game
        self.log.clear();
        self.start_time = Instant::now();
        Ok(())
    }

    fn wait_for_ready(&mut self) -> Result<(), String> {
        let deadline = Instant::now() + Duration::from_millis(self.timeout_ms);
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Err(format!("Engine {} did not respond to Sync", self.name));
            }
            match self.recv_message_timeout(remaining)? {
                Some(EngineMsg::Ready) => return Ok(()),
                Some(EngineMsg::Error(e)) => {
                    eprintln!("[{}] Engine error: {}", self.name, e);
                    // Continue waiting for Ready
                }
                Some(_) | None => {}
            }
        }
    }

    /// Send Play command
    pub fn play(&mut self, moves: Vec<Mv>) -> Result<(), String> {
        self.send_message(&UserMsg::Play(moves))
    }

    /// Send Go command and wait for Best response
    ///
    /// `time_limit_ms` is the time limit given to the engine for its search.
    /// The function will wait up to `self.timeout_ms` (which includes leniency) for a response.
    pub fn go(&mut self, time_limit_ms: u64) -> MoveResult {
        if let Err(_e) = self.send_message(&UserMsg::Go(Limits::time(time_limit_ms))) {
            self.write_log(LogReason::Crash);
            return MoveResult::Crash;
        }

        let start = Instant::now();
        let timeout = Duration::from_millis(self.timeout_ms);

        loop {
            let remaining = timeout.saturating_sub(start.elapsed());
            if remaining.is_zero() {
                self.write_log(LogReason::Timeout);
                return MoveResult::Timeout;
            }

            match self.recv_message_timeout(remaining) {
                Ok(Some(msg)) => match msg {
                    EngineMsg::Best(Some(sq)) => return MoveResult::Move(sq),
                    EngineMsg::Best(None) => return MoveResult::NoMove,
                    EngineMsg::Error(e) => {
                        eprintln!("[{}] Engine error: {}", self.name, e);
                        // Don't treat error messages as fatal, continue waiting
                    }
                    EngineMsg::About { .. } | EngineMsg::Ready => {
                        // Unexpected but not fatal
                    }
                    EngineMsg::Unknown(_) => {
                        // Ignore opaque unknown messages
                    }
                },
                Ok(None) => continue, // Timeout on recv, keep waiting
                Err(e) => {
                    // Don't report errors during graceful shutdown
                    if self.stop_flag.load(Ordering::SeqCst) {
                        return MoveResult::Stopped;
                    }
                    eprintln!("[{}] Communication error: {}", self.name, e);
                    self.write_log(LogReason::Crash);
                    return MoveResult::Crash;
                }
            }
        }
    }

    /// Kill the engine process
    pub fn kill(&mut self) {
        _ = self.child.kill();
        _ = self.child.wait();
    }
}

impl Drop for Engine {
    fn drop(&mut self) {
        self.kill();
    }
}
