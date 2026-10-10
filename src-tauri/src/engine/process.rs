//! A running UCI engine: process control, handshake and searches.
//!
//! Every engine (lc0, Stockfish, or the test fake) goes through this one code
//! path. Output is parsed on a reader task and broadcast to whoever is
//! waiting, so a slow UI never blocks the engine.

use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, Command};
use tokio::sync::broadcast;
use tokio::sync::broadcast::error::RecvError;

use super::uci::{parse_line, InfoLine, UciMessage, UciOption};
use crate::error::{Error, Result};
use crate::game::position::START_FEN;

const STDERR_LINES_KEPT: usize = 200;

/// Result of one `go` command.
#[derive(Debug, Clone, Default)]
pub struct SearchResult {
    pub best: String,
    pub ponder: Option<String>,
    /// Latest search line for each MultiPV index (index 0 = multipv 1).
    pub lines: Vec<InfoLine>,
}

impl SearchResult {
    pub fn top(&self) -> Option<&InfoLine> {
        self.lines.first()
    }
}

pub struct Engine {
    pub path: PathBuf,
    pub name: String,
    pub author: Option<String>,
    pub options: Vec<UciOption>,
    stdin: tokio::sync::Mutex<ChildStdin>,
    child: Mutex<Option<Child>>,
    tx: broadcast::Sender<UciMessage>,
    stderr: Arc<Mutex<VecDeque<String>>>,
    alive: Arc<AtomicBool>,
    search_lock: tokio::sync::Mutex<()>,
}

impl std::fmt::Debug for Engine {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Engine").field("path", &self.path).field("name", &self.name).finish()
    }
}

impl Engine {
    /// Start the engine and complete the `uci` handshake.
    pub async fn spawn(path: &Path, args: &[String]) -> Result<Arc<Engine>> {
        let mut cmd = Command::new(path);
        cmd.args(args).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped()).kill_on_drop(true);
        if let Some(dir) = path.parent() {
            cmd.current_dir(dir);
        }
        let mut child = cmd.spawn().map_err(|e| Error::Engine(format!("cannot start {}: {e}", path.display())))?;
        let mut stdin = child.stdin.take().expect("piped stdin");
        let stdout = child.stdout.take().expect("piped stdout");
        let stderr_pipe = child.stderr.take().expect("piped stderr");

        let (tx, mut rx) = broadcast::channel(8192);
        let alive = Arc::new(AtomicBool::new(true));
        {
            let tx = tx.clone();
            let alive = alive.clone();
            tokio::spawn(async move {
                let mut lines = BufReader::new(stdout).lines();
                while let Ok(Some(line)) = lines.next_line().await {
                    log::trace!("engine> {line}");
                    let _ = tx.send(parse_line(&line));
                }
                alive.store(false, Ordering::SeqCst);
                let _ = tx.send(UciMessage::Exited);
            });
        }
        let stderr = Arc::new(Mutex::new(VecDeque::new()));
        {
            let stderr = stderr.clone();
            tokio::spawn(async move {
                let mut lines = BufReader::new(stderr_pipe).lines();
                while let Ok(Some(line)) = lines.next_line().await {
                    log::debug!("engine stderr> {line}");
                    let mut buf = stderr.lock().unwrap();
                    if buf.len() >= STDERR_LINES_KEPT {
                        buf.pop_front();
                    }
                    buf.push_back(line);
                }
            });
        }

        stdin.write_all(b"uci\n").await?;
        stdin.flush().await?;
        let mut name = None;
        let mut author = None;
        let mut options = Vec::new();
        let handshake = async {
            loop {
                match rx.recv().await {
                    Ok(UciMessage::IdName(n)) => name = Some(n),
                    Ok(UciMessage::IdAuthor(a)) => author = Some(a),
                    Ok(UciMessage::Option(o)) => options.push(o),
                    Ok(UciMessage::UciOk) => return Ok(()),
                    Ok(UciMessage::Exited) | Err(RecvError::Closed) => return Err(Error::Engine("engine exited during startup".into())),
                    Ok(_) | Err(RecvError::Lagged(_)) => {}
                }
            }
        };
        match tokio::time::timeout(Duration::from_secs(20), handshake).await {
            Ok(Ok(())) => {}
            Ok(Err(e)) => {
                let detail = stderr.lock().unwrap().iter().cloned().collect::<Vec<_>>().join("\n");
                return Err(Error::Engine(format!("{e}\n{detail}").trim().to_string()));
            }
            Err(_) => return Err(Error::Engine("engine did not answer the UCI handshake".into())),
        }

        Ok(Arc::new(Engine {
            path: path.to_path_buf(),
            name: name.unwrap_or_else(|| path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()),
            author,
            options,
            stdin: tokio::sync::Mutex::new(stdin),
            child: Mutex::new(Some(child)),
            tx,
            stderr,
            alive,
            search_lock: tokio::sync::Mutex::new(()),
        }))
    }

    pub fn is_alive(&self) -> bool {
        self.alive.load(Ordering::SeqCst)
    }

    pub fn subscribe(&self) -> broadcast::Receiver<UciMessage> {
        self.tx.subscribe()
    }

    pub fn stderr_lines(&self) -> Vec<String> {
        self.stderr.lock().unwrap().iter().cloned().collect()
    }

    pub fn has_option(&self, name: &str) -> bool {
        self.options.iter().any(|o| o.name.eq_ignore_ascii_case(name))
    }

    pub async fn send(&self, command: &str) -> Result<()> {
        if !self.is_alive() {
            return Err(Error::Engine(format!("{} is not running", self.name)));
        }
        log::trace!("engine< {command}");
        let mut stdin = self.stdin.lock().await;
        stdin.write_all(command.as_bytes()).await?;
        stdin.write_all(b"\n").await?;
        stdin.flush().await?;
        Ok(())
    }

    pub async fn set_option(&self, name: &str, value: impl std::fmt::Display) -> Result<()> {
        self.send(&format!("setoption name {name} value {value}")).await
    }

    /// Send `isready` and wait for `readyok`. lc0 loads its network here, which
    /// can take a while on a slow GPU, so callers pass a generous timeout.
    pub async fn ready(&self, timeout: Duration) -> Result<()> {
        let mut rx = self.subscribe();
        self.send("isready").await?;
        let wait = async {
            loop {
                match rx.recv().await {
                    Ok(UciMessage::ReadyOk) => return Ok(()),
                    Ok(UciMessage::Exited) | Err(RecvError::Closed) => return Err(self.exit_error()),
                    _ => {}
                }
            }
        };
        tokio::time::timeout(timeout, wait).await.map_err(|_| Error::Engine(format!("{} did not become ready", self.name)))?
    }

    fn exit_error(&self) -> Error {
        let tail: Vec<String> = self.stderr_lines().into_iter().rev().take(5).collect();
        let detail: Vec<String> = tail.into_iter().rev().collect();
        Error::Engine(format!("{} stopped unexpectedly. {}", self.name, detail.join(" ")).trim().to_string())
    }

    /// Run `position` + `go` and wait for `bestmove`. `on_info` sees every
    /// search line as it arrives. Searches on one engine run one at a time.
    pub async fn search(&self, position: &str, go: &str, on_info: impl FnMut(&InfoLine)) -> Result<SearchResult> {
        self.search_with(&[], position, go, on_info).await
    }

    /// Like [`Engine::search`], but first sends `setup` commands (e.g.
    /// `setoption` for MultiPV) once any previous search has finished, since
    /// UCI doesn't allow changing options mid-search.
    pub async fn search_with(
        &self,
        setup: &[String],
        position: &str,
        go: &str,
        mut on_info: impl FnMut(&InfoLine),
    ) -> Result<SearchResult> {
        let _guard = self.search_lock.lock().await;
        let mut rx = self.subscribe();
        for cmd in setup {
            self.send(cmd).await?;
        }
        self.send(position).await?;
        self.send(go).await?;
        let mut result = SearchResult::default();
        loop {
            match rx.recv().await {
                Ok(UciMessage::Info(info)) if info.is_search_line() => {
                    let idx = info.multipv.unwrap_or(1).max(1) as usize - 1;
                    if result.lines.len() <= idx {
                        result.lines.resize(idx + 1, InfoLine::default());
                    }
                    on_info(&info);
                    result.lines[idx] = info;
                }
                Ok(UciMessage::BestMove { best, ponder }) => {
                    result.best = best;
                    result.ponder = ponder;
                    return Ok(result);
                }
                Ok(UciMessage::Exited) | Err(RecvError::Closed) => return Err(self.exit_error()),
                Ok(_) | Err(RecvError::Lagged(_)) => {}
            }
        }
    }

    pub async fn stop(&self) -> Result<()> {
        self.send("stop").await
    }

    /// Ask the engine to quit, and kill it if it doesn't within half a second.
    pub async fn quit(&self) {
        let _ = self.send("quit").await;
        let child = self.child.lock().unwrap().take();
        if let Some(mut child) = child {
            if tokio::time::timeout(Duration::from_millis(500), child.wait()).await.is_err() {
                let _ = child.start_kill();
            }
        }
        self.alive.store(false, Ordering::SeqCst);
    }

    /// Immediate, synchronous kill for app shutdown.
    pub fn kill(&self) {
        if let Some(mut child) = self.child.lock().unwrap().take() {
            let _ = child.start_kill();
        }
        self.alive.store(false, Ordering::SeqCst);
    }
}

impl Drop for Engine {
    fn drop(&mut self) {
        self.kill();
    }
}

/// `position` command for a start FEN and a list of UCI moves.
pub fn position_command(start_fen: &str, moves: &[String]) -> String {
    let mut cmd = if start_fen == START_FEN { "position startpos".to_string() } else { format!("position fen {start_fen}") };
    if !moves.is_empty() {
        cmd.push_str(" moves ");
        cmd.push_str(&moves.join(" "));
    }
    cmd
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_position_commands() {
        assert_eq!(position_command(START_FEN, &[]), "position startpos");
        assert_eq!(position_command(START_FEN, &["e2e4".into(), "e7e5".into()]), "position startpos moves e2e4 e7e5");
        let fen = "8/8/4k3/8/8/3K4/8/7R w - - 0 1";
        assert_eq!(position_command(fen, &["h1h6".into()]), format!("position fen {fen} moves h1h6"));
    }
}
