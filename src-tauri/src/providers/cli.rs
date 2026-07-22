use super::normalize::{NormalizedEvent, StreamNormalizer};
use crate::model::{Message, ProviderId, ProviderStatus, SessionEvent};
use std::{
    collections::HashSet,
    path::{Path, PathBuf},
    process::Stdio,
    time::Duration,
};
use tauri::{AppHandle, Emitter};
use tokio::{
    io::{AsyncRead, AsyncReadExt},
    process::{Child, Command},
    sync::mpsc,
    time::{sleep, timeout},
};
use tokio_util::sync::CancellationToken;

pub struct ProviderRunResult {
    pub content: String,
    pub provider_session_id: Option<String>,
    pub activities: Vec<Message>,
}

enum ProcessLine {
    Stdout(String),
    Stderr(String),
    Limit(&'static str),
}

pub async fn probe_providers(openrouter_connected: bool) -> Vec<ProviderStatus> {
    let definitions = [
        (
            ProviderId::Claude,
            "https://docs.anthropic.com/en/docs/claude-code",
            "stream-json",
        ),
        (
            ProviderId::Codex,
            "https://developers.openai.com/codex/cli",
            "JSONL (app-server compatible)",
        ),
        (
            ProviderId::Gemini,
            "https://github.com/google-gemini/gemini-cli",
            "stream-json / ACP capable",
        ),
        (
            ProviderId::Kimi,
            "https://moonshotai.github.io/kimi-code/",
            "stream-json / ACP capable",
        ),
    ];
    let mut providers = Vec::new();
    for (id, install_url, transport) in definitions {
        let executable = find_executable(id.command().unwrap_or_default());
        let version = match &executable {
            Some(path) => probe_version(path).await,
            None => None,
        };
        providers.push(ProviderStatus {
            id,
            name: id.display_name().to_string(),
            available: executable.is_some(),
            executable_path: executable.map(|path| path.to_string_lossy().into_owned()),
            version,
            install_url: install_url.to_string(),
            transport: transport.to_string(),
        });
    }
    providers.push(ProviderStatus {
        id: ProviderId::Openrouter,
        name: ProviderId::Openrouter.display_name().to_string(),
        available: openrouter_connected,
        executable_path: None,
        version: None,
        install_url: "https://openrouter.ai/keys".to_string(),
        transport: "HTTPS tool loop".to_string(),
    });
    providers
}

#[allow(clippy::too_many_arguments)]
pub async fn run_cli_turn(
    app: AppHandle,
    provider: ProviderId,
    session_id: String,
    provider_session_id: Option<String>,
    model: Option<String>,
    workspace: PathBuf,
    prompt: String,
    message_id: String,
    cancellation: CancellationToken,
) -> Result<ProviderRunResult, String> {
    let command_name = provider
        .command()
        .ok_or_else(|| "OpenRouter is not a CLI provider".to_string())?;
    let executable = find_executable(command_name).ok_or_else(|| {
        format!(
            "{} is not installed or was not found on PATH",
            provider.display_name()
        )
    })?;
    let args = build_args(
        provider,
        provider_session_id.as_deref(),
        model.as_deref(),
        &prompt,
    );
    let mut command = platform_command(&executable, &args);
    if provider == ProviderId::Gemini {
        // Choosing a canonical workspace in zAI is the explicit trust action.
        command.env("GEMINI_CLI_TRUST_WORKSPACE", "true");
    }
    command
        .current_dir(&workspace)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    configure_process_group(&mut command);

    let mut child = command
        .spawn()
        .map_err(|error| format!("Failed to start {}: {error}", provider.display_name()))?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| "Provider stdout was unavailable".to_string())?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| "Provider stderr was unavailable".to_string())?;
    let (sender, mut receiver) = mpsc::channel::<ProcessLine>(256);

    let stdout_sender = sender.clone();
    tokio::spawn(pump_stream(stdout, stdout_sender, true));
    let stderr_sender = sender.clone();
    tokio::spawn(pump_stream(stderr, stderr_sender, false));
    drop(sender);

    let mut normalizer = StreamNormalizer::new(provider);
    let mut content = String::new();
    let mut provider_thread = provider_session_id;
    let mut activities = Vec::new();
    let mut stderr_tail = String::new();
    let status = loop {
        tokio::select! {
            _ = cancellation.cancelled() => {
                terminate_child(&mut child).await;
                return Err("Turn cancelled".to_string());
            }
            line = receiver.recv() => {
                match line {
                    Some(ProcessLine::Stdout(line)) => {
                        for event in normalizer.parse(&line) {
                            match event {
                                NormalizedEvent::Delta(delta) => {
                                    content.push_str(&delta);
                                    let _ = app.emit("zai://session", SessionEvent::Delta {
                                        session_id: session_id.clone(),
                                        message_id: message_id.clone(),
                                        delta,
                                    });
                                }
                                NormalizedEvent::Text(text) => {
                                    let mut delta = String::new();
                                    if !content.is_empty() && !content.ends_with('\n') && !text.starts_with('\n') {
                                        content.push('\n');
                                        delta.push('\n');
                                    }
                                    content.push_str(&text);
                                    delta.push_str(&text);
                                    let _ = app.emit("zai://session", SessionEvent::Delta {
                                        session_id: session_id.clone(),
                                        message_id: message_id.clone(),
                                        delta,
                                    });
                                }
                                NormalizedEvent::Session(id) => provider_thread = Some(id),
                                NormalizedEvent::Activity(message) => {
                                    let _ = app.emit("zai://session", SessionEvent::Activity {
                                        session_id: session_id.clone(),
                                        message: message.clone(),
                                    });
                                    activities.push(message);
                                }
                            }
                        }
                    }
                    Some(ProcessLine::Stderr(line)) => push_tail(&mut stderr_tail, &line),
                    Some(ProcessLine::Limit(stream)) => {
                        terminate_child(&mut child).await;
                        return Err(format!("{} produced too much {stream} output", provider.display_name()));
                    }
                    None => {
                        if let Some(status) = child.try_wait().map_err(|error| error.to_string())? {
                            break status;
                        }
                    }
                }
            }
            _ = sleep(Duration::from_millis(25)) => {
                if let Some(status) = child.try_wait().map_err(|error| error.to_string())? {
                    if receiver.is_closed() && receiver.is_empty() {
                        break status;
                    }
                }
            }
        }
    };

    if !status.success() {
        let detail = stderr_tail.trim();
        return Err(if detail.is_empty() {
            format!("{} exited with {status}", provider.display_name())
        } else {
            format!("{} exited with {status}: {detail}", provider.display_name())
        });
    }
    if content.trim().is_empty() {
        return Err(format!(
            "{} completed without returning a message",
            provider.display_name()
        ));
    }
    Ok(ProviderRunResult {
        content: content.trim().to_string(),
        provider_session_id: provider_thread,
        activities,
    })
}

fn build_args(
    provider: ProviderId,
    provider_session_id: Option<&str>,
    model: Option<&str>,
    prompt: &str,
) -> Vec<String> {
    let model = model.filter(|value| !value.is_empty() && *value != "default");
    match provider {
        ProviderId::Claude => {
            let mut args = vec![
                "--print".to_string(),
                "--verbose".to_string(),
                "--output-format".to_string(),
                "stream-json".to_string(),
                "--include-partial-messages".to_string(),
                "--permission-mode".to_string(),
                "acceptEdits".to_string(),
            ];
            if let Some(id) = provider_session_id {
                args.extend(["--resume".to_string(), id.to_string()]);
            }
            if let Some(model) = model {
                args.extend(["--model".to_string(), model.to_string()]);
            }
            args.push(prompt.to_string());
            args
        }
        ProviderId::Codex => {
            let mut args = vec!["exec".to_string()];
            if let Some(id) = provider_session_id {
                args.extend([
                    "resume".to_string(),
                    "--json".to_string(),
                    "--skip-git-repo-check".to_string(),
                ]);
                if let Some(model) = model {
                    args.extend(["--model".to_string(), model.to_string()]);
                }
                args.extend([id.to_string(), prompt.to_string()]);
            } else {
                args.extend([
                    "--json".to_string(),
                    "--sandbox".to_string(),
                    "workspace-write".to_string(),
                    "--skip-git-repo-check".to_string(),
                ]);
                if let Some(model) = model {
                    args.extend(["--model".to_string(), model.to_string()]);
                }
                args.push(prompt.to_string());
            }
            args
        }
        ProviderId::Gemini => {
            let mut args = vec![
                "--prompt".to_string(),
                prompt.to_string(),
                "--output-format".to_string(),
                "stream-json".to_string(),
                "--approval-mode".to_string(),
                "auto_edit".to_string(),
            ];
            if let Some(id) = provider_session_id {
                args.extend(["--resume".to_string(), id.to_string()]);
            }
            if let Some(model) = model {
                args.extend(["--model".to_string(), model.to_string()]);
            }
            args
        }
        ProviderId::Kimi => {
            let mut args = vec![
                "--prompt".to_string(),
                prompt.to_string(),
                "--output-format".to_string(),
                "stream-json".to_string(),
            ];
            if let Some(id) = provider_session_id {
                args.extend(["--session".to_string(), id.to_string()]);
            }
            if let Some(model) = model {
                args.extend(["--model".to_string(), model.to_string()]);
            }
            args
        }
        ProviderId::Openrouter => Vec::new(),
    }
}

fn find_executable(command: &str) -> Option<PathBuf> {
    if command.is_empty() {
        return None;
    }
    if let Ok(path) = which::which(command) {
        return Some(path);
    }
    let home = dirs::home_dir();
    let mut directories = vec![
        PathBuf::from("/opt/homebrew/bin"),
        PathBuf::from("/usr/local/bin"),
        PathBuf::from("/usr/bin"),
    ];
    if let Some(home) = home {
        directories.extend([
            home.join(".local/bin"),
            home.join(".local/share/pnpm"),
            home.join(".npm-global/bin"),
            home.join(".kimi-code/bin"),
            home.join(".cargo/bin"),
        ]);
        #[cfg(windows)]
        directories.push(home.join("AppData/Roaming/npm"));
    }
    let mut candidates = HashSet::new();
    for directory in directories {
        candidates.insert(directory.join(command));
        #[cfg(windows)]
        {
            candidates.insert(directory.join(format!("{command}.exe")));
            candidates.insert(directory.join(format!("{command}.cmd")));
        }
    }
    candidates.into_iter().find(|path| path.is_file())
}

async fn probe_version(path: &Path) -> Option<String> {
    let mut command = platform_command(path, &["--version".to_string()]);
    command
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    let output = timeout(Duration::from_secs(4), command.output())
        .await
        .ok()?
        .ok()?;
    let value = if output.stdout.is_empty() {
        String::from_utf8_lossy(&output.stderr).into_owned()
    } else {
        String::from_utf8_lossy(&output.stdout).into_owned()
    };
    value
        .lines()
        .find(|line| !line.trim().is_empty())
        .map(|line| line.trim().to_string())
}

fn platform_command(path: &Path, args: &[String]) -> Command {
    #[cfg(windows)]
    {
        let extension = path
            .extension()
            .and_then(|value| value.to_str())
            .unwrap_or_default();
        if extension.eq_ignore_ascii_case("cmd") || extension.eq_ignore_ascii_case("bat") {
            let mut command = Command::new("cmd.exe");
            command.args(["/D", "/S", "/C"]).arg(path).args(args);
            return command;
        }
    }
    let mut command = Command::new(path);
    command.args(args);
    command
}

fn configure_process_group(command: &mut Command) {
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.as_std_mut().process_group(0);
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
        command
            .as_std_mut()
            .creation_flags(CREATE_NEW_PROCESS_GROUP);
    }
}

async fn pump_stream<R>(mut reader: R, sender: mpsc::Sender<ProcessLine>, stdout: bool)
where
    R: AsyncRead + Unpin,
{
    const MAX_LINE: usize = 1024 * 1024;
    const MAX_STREAM: usize = 32 * 1024 * 1024;
    let stream_name = if stdout { "stdout" } else { "stderr" };
    let mut pending = Vec::new();
    let mut buffer = [0_u8; 8192];
    let mut total = 0_usize;
    loop {
        let read = match reader.read(&mut buffer).await {
            Ok(0) => break,
            Ok(read) => read,
            Err(_) => return,
        };
        total = total.saturating_add(read);
        if total > MAX_STREAM {
            let _ = sender.send(ProcessLine::Limit(stream_name)).await;
            return;
        }
        pending.extend_from_slice(&buffer[..read]);
        while let Some(index) = pending.iter().position(|byte| *byte == b'\n') {
            let mut line = pending.drain(..=index).collect::<Vec<_>>();
            if line.last() == Some(&b'\n') {
                line.pop();
            }
            if line.last() == Some(&b'\r') {
                line.pop();
            }
            let value = String::from_utf8_lossy(&line).into_owned();
            let item = if stdout {
                ProcessLine::Stdout(value)
            } else {
                ProcessLine::Stderr(value)
            };
            if sender.send(item).await.is_err() {
                return;
            }
        }
        if pending.len() > MAX_LINE {
            let _ = sender.send(ProcessLine::Limit(stream_name)).await;
            return;
        }
    }
    if !pending.is_empty() {
        let value = String::from_utf8_lossy(&pending).into_owned();
        let item = if stdout {
            ProcessLine::Stdout(value)
        } else {
            ProcessLine::Stderr(value)
        };
        let _ = sender.send(item).await;
    }
}

async fn terminate_child(child: &mut Child) {
    let pid = child.id();
    if let Some(pid) = pid {
        #[cfg(unix)]
        unsafe {
            libc::kill(-(pid as i32), libc::SIGTERM);
        }
        #[cfg(windows)]
        {
            let _ = Command::new("taskkill")
                .args(["/PID", &pid.to_string(), "/T", "/F"])
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status()
                .await;
        }
    }
    #[cfg(unix)]
    {
        sleep(Duration::from_millis(400)).await;
        if let Some(pid) = pid {
            unsafe {
                libc::kill(-(pid as i32), libc::SIGKILL);
            }
        }
    }
    if timeout(Duration::from_secs(2), child.wait()).await.is_err() {
        let _ = child.start_kill();
        let _ = child.wait().await;
    }
}

fn push_tail(target: &mut String, line: &str) {
    const MAX: usize = 8 * 1024;
    target.push_str(line);
    target.push('\n');
    if target.len() > MAX {
        let mut start = target.len() - MAX;
        while start < target.len() && !target.is_char_boundary(start) {
            start += 1;
        }
        *target = target[start..].to_string();
    }
}

#[cfg(test)]
mod tests {
    use super::{build_args, push_tail};
    use crate::model::ProviderId;

    #[test]
    fn codex_resume_uses_documented_subcommand() {
        assert_eq!(
            build_args(ProviderId::Codex, Some("thread"), None, "continue"),
            [
                "exec",
                "resume",
                "--json",
                "--skip-git-repo-check",
                "thread",
                "continue"
            ]
        );
    }

    #[test]
    fn no_provider_uses_a_shell_string() {
        for provider in [
            ProviderId::Claude,
            ProviderId::Codex,
            ProviderId::Gemini,
            ProviderId::Kimi,
        ] {
            let args = build_args(provider, None, None, "hello; rm -rf ignored");
            assert!(args.iter().any(|arg| arg == "hello; rm -rf ignored"));
        }
    }

    #[test]
    fn gemini_headless_mode_allows_edits() {
        let args = build_args(ProviderId::Gemini, None, None, "fix it");
        assert!(
            args.windows(2)
                .any(|pair| pair == ["--approval-mode", "auto_edit"])
        );
    }

    #[test]
    fn stderr_tail_truncates_multibyte_text() {
        let mut value = "é".repeat(5_000);
        push_tail(&mut value, "done");
        assert!(value.len() <= 8 * 1024);
        assert!(value.ends_with("done\n"));
    }
}
