use crate::{providers::process::find_executable, workspace::canonical_workspace};
use parking_lot::Mutex as ParkingMutex;
use portable_pty::{Child, ChildKiller, CommandBuilder, MasterPty, PtySize, native_pty_system};
use serde::Serialize;
use std::{
    collections::{HashMap, HashSet},
    ffi::OsStr,
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex as StdMutex,
        atomic::{AtomicBool, Ordering},
    },
};
use tauri::{AppHandle, Emitter};
use uuid::Uuid;

const MAX_TERMINALS: usize = 12;
const MAX_WRITE_BYTES: usize = 64 * 1024;
const MAX_STREAM_BYTES: usize = 64 * 1024 * 1024;
const READ_CHUNK_BYTES: usize = 8 * 1024;

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalSession {
    pub id: String,
    pub cwd: String,
    pub shell: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct TerminalEvent {
    session_id: String,
    kind: String,
    data: Option<String>,
    exit_code: Option<u32>,
}

#[derive(Clone, Default)]
pub struct TerminalRegistry {
    sessions: Arc<ParkingMutex<HashMap<String, Arc<TerminalHandle>>>>,
}

struct TerminalHandle {
    master: Arc<StdMutex<Option<Box<dyn MasterPty + Send>>>>,
    writer: Arc<StdMutex<Option<Box<dyn Write + Send>>>>,
    killer: Arc<StdMutex<Box<dyn ChildKiller + Send + Sync>>>,
    process_id: Option<u32>,
    closed: AtomicBool,
}

impl TerminalHandle {
    fn terminate(&self) {
        if self.closed.swap(true, Ordering::SeqCst) {
            return;
        }

        #[cfg(unix)]
        {
            let foreground_group = self.master.lock().ok().and_then(|master| {
                master
                    .as_ref()
                    .and_then(|master| master.process_group_leader())
            });
            let own_group = unsafe { libc::getpgrp() };
            let foreground_group =
                foreground_group.filter(|group| *group > 1 && *group != own_group);
            if let Some(group) = foreground_group {
                // The shell owns a fresh session and the active command normally owns the
                // foreground process group. Signal that complete group before stopping the
                // shell itself so Ctrl+C-style descendants do not outlive the terminal tab.
                unsafe {
                    libc::kill(-group, libc::SIGTERM);
                }
            }
            let shell_group = self
                .process_id
                .map(|pid| pid as i32)
                .filter(|group| *group > 1 && *group != own_group);
            if let Some(group) = shell_group {
                unsafe {
                    libc::kill(-group, libc::SIGTERM);
                }
            }
            self.close_io();
            std::thread::sleep(std::time::Duration::from_millis(150));
            for group in [foreground_group, shell_group].into_iter().flatten() {
                unsafe {
                    libc::kill(-group, libc::SIGKILL);
                }
            }
            if let Some(pid) = self.process_id.filter(|pid| *pid > 1) {
                unsafe {
                    libc::kill(pid as i32, libc::SIGKILL);
                }
            }
        }

        #[cfg(windows)]
        {
            if let Some(pid) = self.process_id {
                let _ = std::process::Command::new("taskkill.exe")
                    .args(["/PID", &pid.to_string(), "/T", "/F"])
                    .stdin(std::process::Stdio::null())
                    .stdout(std::process::Stdio::null())
                    .stderr(std::process::Stdio::null())
                    .status();
            }
            self.close_io();
        }

        if let Ok(mut killer) = self.killer.lock() {
            let _ = killer.kill();
        }
    }

    fn close_io(&self) {
        if let Ok(mut writer) = self.writer.lock() {
            writer.take();
        }
        if let Ok(mut master) = self.master.lock() {
            master.take();
        }
    }
}

struct SpawnedPty {
    shell: String,
    master: Box<dyn MasterPty + Send>,
    reader: Box<dyn Read + Send>,
    writer: Box<dyn Write + Send>,
    child: Box<dyn Child + Send + Sync>,
}

#[derive(Clone, Debug)]
struct ShellCandidate {
    path: PathBuf,
    args: Vec<String>,
}

impl TerminalRegistry {
    pub async fn open(
        &self,
        app: AppHandle,
        workspace: String,
        cols: u16,
        rows: u16,
        wsl_distribution: Option<String>,
    ) -> Result<TerminalSession, String> {
        validate_size(cols, rows)?;
        let workspace = canonical_workspace(&workspace)?;
        if self.sessions.lock().len() >= MAX_TERMINALS {
            return Err(format!(
                "At most {MAX_TERMINALS} terminal sessions may be open"
            ));
        }

        let spawn_workspace = workspace.clone();
        let requested_wsl = wsl_distribution.clone();
        let spawned = tokio::task::spawn_blocking(move || {
            spawn_pty(
                &spawn_workspace,
                PtySize {
                    rows,
                    cols,
                    pixel_width: 0,
                    pixel_height: 0,
                },
                requested_wsl.as_deref(),
            )
        })
        .await
        .map_err(|error| format!("Terminal startup task failed: {error}"))??;

        let session_id = Uuid::new_v4().to_string();
        let shell = spawned.shell.clone();
        let cwd = workspace.to_string_lossy().into_owned();
        let process_id = spawned.child.process_id();
        let handle = Arc::new(TerminalHandle {
            master: Arc::new(StdMutex::new(Some(spawned.master))),
            writer: Arc::new(StdMutex::new(Some(spawned.writer))),
            killer: Arc::new(StdMutex::new(spawned.child.clone_killer())),
            process_id,
            closed: AtomicBool::new(false),
        });

        {
            let mut sessions = self.sessions.lock();
            if sessions.len() >= MAX_TERMINALS {
                handle.terminate();
                return Err(format!(
                    "At most {MAX_TERMINALS} terminal sessions may be open"
                ));
            }
            sessions.insert(session_id.clone(), handle.clone());
        }

        if let Err(error) = self.spawn_reader(
            app.clone(),
            session_id.clone(),
            spawned.reader,
            handle.clone(),
        ) {
            self.sessions.lock().remove(&session_id);
            handle.terminate();
            return Err(error);
        }
        if let Err(error) = self.spawn_waiter(app, session_id.clone(), spawned.child) {
            self.sessions.lock().remove(&session_id);
            handle.terminate();
            return Err(error);
        }

        Ok(TerminalSession {
            id: session_id,
            cwd,
            shell,
        })
    }

    pub async fn write(&self, session_id: String, data: String) -> Result<(), String> {
        if data.len() > MAX_WRITE_BYTES {
            return Err(format!(
                "A terminal write cannot exceed {} KiB",
                MAX_WRITE_BYTES / 1024
            ));
        }
        let handle = self
            .sessions
            .lock()
            .get(&session_id)
            .cloned()
            .ok_or_else(|| "Terminal session is closed".to_string())?;
        if handle.closed.load(Ordering::SeqCst) {
            return Err("Terminal session is closed".to_string());
        }
        tokio::task::spawn_blocking(move || {
            let mut writer = handle
                .writer
                .lock()
                .map_err(|_| "Terminal input lock is unavailable".to_string())?;
            let writer = writer
                .as_mut()
                .ok_or_else(|| "Terminal session is closed".to_string())?;
            writer
                .write_all(data.as_bytes())
                .and_then(|()| writer.flush())
                .map_err(|error| format!("Unable to write to terminal: {error}"))
        })
        .await
        .map_err(|error| format!("Terminal input task failed: {error}"))?
    }

    pub async fn resize(&self, session_id: String, cols: u16, rows: u16) -> Result<(), String> {
        validate_size(cols, rows)?;
        let handle = self
            .sessions
            .lock()
            .get(&session_id)
            .cloned()
            .ok_or_else(|| "Terminal session is closed".to_string())?;
        tokio::task::spawn_blocking(move || {
            let master = handle
                .master
                .lock()
                .map_err(|_| "Terminal resize lock is unavailable".to_string())?;
            let master = master
                .as_ref()
                .ok_or_else(|| "Terminal session is closed".to_string())?;
            master
                .resize(PtySize {
                    rows,
                    cols,
                    pixel_width: 0,
                    pixel_height: 0,
                })
                .map_err(|error| format!("Unable to resize terminal: {error}"))
        })
        .await
        .map_err(|error| format!("Terminal resize task failed: {error}"))?
    }

    pub async fn close(&self, session_id: String) -> Result<(), String> {
        let Some(handle) = self.sessions.lock().remove(&session_id) else {
            return Ok(());
        };
        tokio::task::spawn_blocking(move || handle.terminate())
            .await
            .map_err(|error| format!("Terminal shutdown task failed: {error}"))?;
        Ok(())
    }

    pub fn has_sessions(&self) -> bool {
        !self.sessions.lock().is_empty()
    }

    pub fn shutdown_all(&self) {
        let handles = self
            .sessions
            .lock()
            .drain()
            .map(|(_, handle)| handle)
            .collect::<Vec<_>>();
        for handle in handles {
            handle.terminate();
        }
    }

    fn spawn_reader(
        &self,
        app: AppHandle,
        session_id: String,
        mut reader: Box<dyn Read + Send>,
        handle: Arc<TerminalHandle>,
    ) -> Result<(), String> {
        std::thread::Builder::new()
            .name(format!("onyx-pty-read-{}", short_id(&session_id)))
            .spawn(move || {
                let mut decoder = Utf8StreamDecoder::default();
                let mut total = 0_usize;
                let mut buffer = [0_u8; READ_CHUNK_BYTES];
                loop {
                    match reader.read(&mut buffer) {
                        Ok(0) => {
                            let tail = decoder.finish();
                            if !tail.is_empty() {
                                emit_data(&app, &session_id, tail);
                            }
                            break;
                        }
                        Ok(read) => {
                            total = total.saturating_add(read);
                            if total > MAX_STREAM_BYTES {
                                emit_error(
                                    &app,
                                    &session_id,
                                    format!(
                                        "Terminal output exceeded the {} MiB session limit",
                                        MAX_STREAM_BYTES / 1024 / 1024
                                    ),
                                );
                                handle.terminate();
                                break;
                            }
                            let data = decoder.push(&buffer[..read]);
                            if !data.is_empty() {
                                emit_data(&app, &session_id, data);
                            }
                        }
                        Err(error) => {
                            #[cfg(unix)]
                            let expected_pty_close = error.raw_os_error() == Some(libc::EIO);
                            #[cfg(not(unix))]
                            let expected_pty_close = false;
                            if !expected_pty_close && !handle.closed.load(Ordering::SeqCst) {
                                emit_error(
                                    &app,
                                    &session_id,
                                    format!("Terminal output stopped: {error}"),
                                );
                            }
                            break;
                        }
                    }
                }
            })
            .map_err(|error| format!("Unable to start terminal output reader: {error}"))?;
        Ok(())
    }

    fn spawn_waiter(
        &self,
        app: AppHandle,
        session_id: String,
        mut child: Box<dyn Child + Send + Sync>,
    ) -> Result<(), String> {
        let registry = self.clone();
        std::thread::Builder::new()
            .name(format!("onyx-pty-wait-{}", short_id(&session_id)))
            .spawn(move || {
                let result = child.wait();
                registry.sessions.lock().remove(&session_id);
                match result {
                    Ok(status) => {
                        let _ = app.emit(
                            "onyx://terminal",
                            TerminalEvent {
                                session_id,
                                kind: "exit".to_string(),
                                data: None,
                                exit_code: Some(status.exit_code()),
                            },
                        );
                    }
                    Err(error) => emit_error(
                        &app,
                        &session_id,
                        format!("Unable to wait for terminal process: {error}"),
                    ),
                }
            })
            .map_err(|error| format!("Unable to start terminal process waiter: {error}"))?;
        Ok(())
    }
}

fn spawn_pty(
    workspace: &Path,
    size: PtySize,
    wsl_distribution: Option<&str>,
) -> Result<SpawnedPty, String> {
    let candidates = shell_candidates(wsl_distribution)?;
    if candidates.is_empty() {
        return Err("No supported terminal shell was found".to_string());
    }
    let mut failures = Vec::new();
    for candidate in &candidates {
        match spawn_candidate(workspace, size, candidate) {
            Ok(spawned) => return Ok(spawned),
            Err(error) => failures.push(format!("{}: {error}", candidate.path.display())),
        }
    }
    Err(format!(
        "Unable to start a terminal with any available shell ({})",
        failures.join("; ")
    ))
}

fn spawn_candidate(
    workspace: &Path,
    size: PtySize,
    candidate: &ShellCandidate,
) -> Result<SpawnedPty, String> {
    let pty_system = native_pty_system();
    let pair = pty_system
        .openpty(size)
        .map_err(|error| format!("PTY allocation failed: {error}"))?;
    let mut command = CommandBuilder::new(&candidate.path);
    command.args(&candidate.args);
    command.cwd(workspace);
    command.env("TERM", "xterm-256color");
    command.env("COLORTERM", "truecolor");
    command.env("ONYX_TERMINAL", "1");
    command.env_remove("VITE_DEV_SERVER_URL");
    let child = pair
        .slave
        .spawn_command(command)
        .map_err(|error| format!("Shell spawn failed: {error}"))?;
    let reader = pair
        .master
        .try_clone_reader()
        .map_err(|error| format!("PTY output setup failed: {error}"))?;
    let writer = pair
        .master
        .take_writer()
        .map_err(|error| format!("PTY input setup failed: {error}"))?;
    drop(pair.slave);
    Ok(SpawnedPty {
        shell: candidate.path.to_string_lossy().into_owned(),
        master: pair.master,
        reader,
        writer,
        child,
    })
}

fn shell_candidates(wsl_distribution: Option<&str>) -> Result<Vec<ShellCandidate>, String> {
    #[cfg(not(windows))]
    let _ = wsl_distribution;
    let mut paths = Vec::new();
    #[cfg(unix)]
    {
        if let Some(shell) = std::env::var_os("SHELL").filter(|value| !value.is_empty()) {
            paths.push(PathBuf::from(shell));
        }
        paths.extend(["/bin/zsh", "/bin/bash", "/bin/sh"].map(PathBuf::from));
    }
    #[cfg(windows)]
    {
        if let Some(distribution) = wsl_distribution {
            if distribution.len() > 100 || distribution.chars().any(char::is_control) {
                return Err("The selected WSL distribution name is invalid".into());
            }
            let wsl = find_executable("wsl.exe")
                .ok_or_else(|| "WSL is not installed or wsl.exe is unavailable".to_string())?;
            return Ok(vec![ShellCandidate {
                path: wsl,
                args: if distribution.is_empty() {
                    Vec::new()
                } else {
                    vec!["--distribution".into(), distribution.into()]
                },
            }]);
        }
        for command in ["pwsh.exe", "powershell.exe"] {
            if let Some(path) = find_executable(command) {
                paths.push(path);
            }
        }
        if let Some(comspec) = std::env::var_os("ComSpec").filter(|value| !value.is_empty()) {
            paths.push(PathBuf::from(comspec));
        }
        if let Some(path) = find_executable("cmd.exe") {
            paths.push(path);
        }
    }

    let mut seen = HashSet::new();
    Ok(paths
        .into_iter()
        .filter_map(|path| {
            let resolved = if path.is_file() {
                path
            } else {
                find_executable(path.to_str()?)?
            };
            let key = resolved.to_string_lossy().to_lowercase();
            seen.insert(key).then(|| ShellCandidate {
                args: shell_args(&resolved),
                path: resolved,
            })
        })
        .collect())
}

fn shell_args(path: &Path) -> Vec<String> {
    let name = path
        .file_name()
        .and_then(OsStr::to_str)
        .unwrap_or_default()
        .to_ascii_lowercase();
    if name == "zsh" {
        vec!["-o".to_string(), "nopromptsp".to_string()]
    } else if matches!(name.as_str(), "pwsh" | "pwsh.exe" | "powershell.exe") {
        vec!["-NoLogo".to_string()]
    } else {
        Vec::new()
    }
}

fn validate_size(cols: u16, rows: u16) -> Result<(), String> {
    if !(2..=500).contains(&cols) || !(1..=300).contains(&rows) {
        return Err("Terminal size must be 2-500 columns and 1-300 rows".to_string());
    }
    Ok(())
}

fn emit_data(app: &AppHandle, session_id: &str, data: String) {
    let _ = app.emit(
        "onyx://terminal",
        TerminalEvent {
            session_id: session_id.to_string(),
            kind: "data".to_string(),
            data: Some(data),
            exit_code: None,
        },
    );
}

fn emit_error(app: &AppHandle, session_id: &str, data: String) {
    let _ = app.emit(
        "onyx://terminal",
        TerminalEvent {
            session_id: session_id.to_string(),
            kind: "error".to_string(),
            data: Some(data),
            exit_code: None,
        },
    );
}

fn short_id(value: &str) -> &str {
    value.get(..8).unwrap_or(value)
}

#[derive(Default)]
struct Utf8StreamDecoder {
    pending: Vec<u8>,
}

impl Utf8StreamDecoder {
    fn push(&mut self, bytes: &[u8]) -> String {
        self.pending.extend_from_slice(bytes);
        self.decode(false)
    }

    fn finish(&mut self) -> String {
        self.decode(true)
    }

    fn decode(&mut self, final_chunk: bool) -> String {
        let mut output = String::new();
        loop {
            match std::str::from_utf8(&self.pending) {
                Ok(value) => {
                    output.push_str(value);
                    self.pending.clear();
                    break;
                }
                Err(error) => {
                    let valid = error.valid_up_to();
                    if valid > 0 {
                        // `valid_up_to` guarantees this prefix is UTF-8.
                        output.push_str(std::str::from_utf8(&self.pending[..valid]).unwrap_or(""));
                        self.pending.drain(..valid);
                    }
                    match error.error_len() {
                        Some(length) => {
                            let length = length.min(self.pending.len());
                            output.push_str(&String::from_utf8_lossy(&self.pending[..length]));
                            self.pending.drain(..length);
                        }
                        None if final_chunk => {
                            output.push_str(&String::from_utf8_lossy(&self.pending));
                            self.pending.clear();
                            break;
                        }
                        None => break,
                    }
                }
            }
        }
        output
    }
}

#[cfg(test)]
mod tests {
    use super::{
        ShellCandidate, TerminalRegistry, Utf8StreamDecoder, shell_args, spawn_candidate,
        validate_size,
    };
    use portable_pty::PtySize;
    use std::{
        io::{Read, Write},
        path::{Path, PathBuf},
        sync::mpsc,
        time::Duration,
    };

    #[test]
    fn streaming_utf8_decoder_preserves_split_codepoints() {
        let mut decoder = Utf8StreamDecoder::default();
        assert_eq!(decoder.push(&[0xe2, 0x82]), "");
        assert_eq!(decoder.push(&[0xac, b'!']), "€!");
        assert_eq!(decoder.finish(), "");
    }

    #[test]
    fn streaming_utf8_decoder_replaces_invalid_bytes() {
        let mut decoder = Utf8StreamDecoder::default();
        assert_eq!(decoder.push(&[0xff, b'x']), "�x");
    }

    #[test]
    fn validates_terminal_dimensions() {
        assert!(validate_size(80, 24).is_ok());
        assert!(validate_size(1, 24).is_err());
        assert!(validate_size(80, 301).is_err());
    }

    #[test]
    fn applies_shell_specific_noninteractive_noise_reduction() {
        assert_eq!(shell_args(Path::new("/bin/zsh")), ["-o", "nopromptsp"]);
        assert_eq!(shell_args(Path::new("pwsh.exe")), ["-NoLogo"]);
        assert!(shell_args(Path::new("/bin/bash")).is_empty());
    }

    #[tokio::test]
    async fn closing_an_absent_terminal_is_idempotent() {
        TerminalRegistry::default()
            .close("already-exited".to_string())
            .await
            .expect("closing an absent terminal should succeed");
    }

    #[cfg(unix)]
    #[test]
    fn native_pty_round_trips_shell_output() {
        let candidate = ShellCandidate {
            path: PathBuf::from("/bin/sh"),
            args: Vec::new(),
        };
        let mut spawned = spawn_candidate(
            Path::new("/tmp"),
            PtySize {
                rows: 24,
                cols: 80,
                pixel_width: 0,
                pixel_height: 0,
            },
            &candidate,
        )
        .expect("PTY should start /bin/sh");
        let process_id = spawned.child.process_id();
        let mut child = spawned.child;
        let mut reader = spawned.reader;
        let (sender, receiver) = mpsc::channel();
        std::thread::spawn(move || {
            let mut output = Vec::new();
            let mut buffer = [0_u8; 1024];
            while output.len() < 64 * 1024 {
                match reader.read(&mut buffer) {
                    Ok(0) | Err(_) => break,
                    Ok(read) => {
                        output.extend_from_slice(&buffer[..read]);
                        if output
                            .windows(b"__ZAI_PTY_OK__".len())
                            .any(|window| window == b"__ZAI_PTY_OK__")
                        {
                            break;
                        }
                    }
                }
            }
            let _ = sender.send(output);
        });
        spawned
            .writer
            .write_all(b"printf '__ZAI_'; printf 'PTY_OK__\\n'; exit\n")
            .expect("PTY input should accept a shell command");
        spawned.writer.flush().expect("PTY input should flush");
        let output = receiver
            .recv_timeout(Duration::from_secs(5))
            .unwrap_or_else(|error| {
                let _ = child.kill();
                panic!("PTY output timed out: {error}")
            });
        assert!(String::from_utf8_lossy(&output).contains("__ZAI_PTY_OK__"));
        drop(spawned.writer);
        drop(spawned.master);
        if let Some(process_id) = process_id {
            unsafe {
                libc::kill(-(process_id as i32), libc::SIGKILL);
                libc::kill(process_id as i32, libc::SIGKILL);
            }
        }
        let mut exited = false;
        for _ in 0..100 {
            if child.try_wait().expect("poll PTY shell").is_some() {
                exited = true;
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        if !exited {
            let _ = child.kill();
            exited = child.try_wait().expect("poll killed PTY shell").is_some();
        }
        assert!(exited, "PTY shell should stop after the lifecycle test");
    }
}
