use super::{
    driver::{
        DriverFuture, ProviderActivity, ProviderDriver, ProviderEvent, ProviderSession,
        ProviderSessionConfig,
    },
    normalize::{NormalizedEvent, StreamNormalizer},
    process::{JsonProcess, ProcessOutput, find_executable, platform_command, probe_version},
};
use crate::model::{MessageKind, ProviderId, ProviderStatus};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

const MAX_ONE_SHOT_STREAM: usize = 32 * 1024 * 1024;

pub async fn probe_providers(openrouter_connected: bool) -> Vec<ProviderStatus> {
    let definitions = [
        (
            ProviderId::Claude,
            "https://docs.anthropic.com/en/docs/claude-code",
            "persistent stream-json",
        ),
        (
            ProviderId::Codex,
            "https://developers.openai.com/codex/cli",
            "persistent app-server JSON-RPC",
        ),
        (
            ProviderId::Gemini,
            "https://github.com/google-gemini/gemini-cli",
            "bounded stream-json",
        ),
        (
            ProviderId::Kimi,
            "https://moonshotai.github.io/kimi-code/",
            "bounded stream-json",
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

pub struct CliDriver {
    provider: ProviderId,
}

impl CliDriver {
    pub fn new(provider: ProviderId) -> Self {
        Self { provider }
    }
}

impl ProviderDriver for CliDriver {
    fn provider(&self) -> ProviderId {
        self.provider
    }

    fn connect<'a>(
        &'a self,
        config: ProviderSessionConfig,
    ) -> DriverFuture<'a, Result<Box<dyn ProviderSession>, String>> {
        Box::pin(async move {
            let command = config
                .provider
                .command()
                .ok_or_else(|| "OpenRouter is not a CLI provider".to_string())?;
            if find_executable(command).is_none() {
                return Err(format!(
                    "{} is not installed or was not found on PATH",
                    config.provider.display_name()
                ));
            }
            Ok(Box::new(CliSession {
                continuation: config.continuation.clone(),
                config,
                startup_activity: None,
            }) as Box<dyn ProviderSession>)
        })
    }
}

pub struct CliSession {
    config: ProviderSessionConfig,
    continuation: Option<String>,
    startup_activity: Option<String>,
}

impl CliSession {
    pub fn with_fallback_notice(config: ProviderSessionConfig, detail: String) -> Self {
        Self {
            continuation: config.continuation.clone(),
            config,
            startup_activity: Some(detail),
        }
    }
}

impl ProviderSession for CliSession {
    fn provider(&self) -> ProviderId {
        self.config.provider
    }

    fn continuation(&self) -> Option<String> {
        self.continuation.clone()
    }

    fn run_turn<'a>(
        &'a mut self,
        prompt: &'a str,
        cancellation: &'a CancellationToken,
        events: mpsc::Sender<ProviderEvent>,
    ) -> DriverFuture<'a, Result<(), String>> {
        Box::pin(async move {
            if let Some(detail) = self.startup_activity.take() {
                send_event(
                    &events,
                    ProviderEvent::Activity(ProviderActivity {
                        title: "Using compatibility transport".to_string(),
                        detail: Some(detail),
                        kind: MessageKind::Text,
                    }),
                )
                .await?;
            }
            let continuation = self.continuation.clone();
            run_one_shot(
                &self.config,
                continuation.as_deref(),
                prompt,
                cancellation,
                &events,
                &mut self.continuation,
            )
            .await
        })
    }

    fn shutdown<'a>(&'a mut self) -> DriverFuture<'a, ()> {
        Box::pin(async {})
    }
}

async fn run_one_shot(
    config: &ProviderSessionConfig,
    continuation: Option<&str>,
    prompt: &str,
    cancellation: &CancellationToken,
    events: &mpsc::Sender<ProviderEvent>,
    next_continuation: &mut Option<String>,
) -> Result<(), String> {
    let provider = config.provider;
    let command_name = provider
        .command()
        .ok_or_else(|| "OpenRouter is not a CLI provider".to_string())?;
    let executable = find_executable(command_name).ok_or_else(|| {
        format!(
            "{} is not installed or was not found on PATH",
            provider.display_name()
        )
    })?;
    let args = build_args(provider, continuation, config.model(), prompt);
    let mut command = platform_command(&executable, &args);
    if provider == ProviderId::Gemini {
        command.env("GEMINI_CLI_TRUST_WORKSPACE", "true");
    }
    command.current_dir(&config.workspace);
    let mut process =
        JsonProcess::spawn(command, provider.display_name(), MAX_ONE_SHOT_STREAM).await?;
    process.close_stdin().await?;
    let mut normalizer = StreamNormalizer::new(provider);

    let status = loop {
        tokio::select! {
            _ = cancellation.cancelled() => {
                process.shutdown().await;
                return Err("Turn cancelled".to_string());
            }
            output = process.next_stdout() => {
                match output? {
                    ProcessOutput::Stdout(line) => {
                        for event in normalizer.parse(&line) {
                            match event {
                                NormalizedEvent::Delta(delta) => {
                                    send_event(events, ProviderEvent::TextDelta(delta)).await?;
                                }
                                NormalizedEvent::Text(text) => {
                                    send_event(events, ProviderEvent::Text(text)).await?;
                                }
                                NormalizedEvent::Session(id) => {
                                    *next_continuation = Some(id.clone());
                                    send_event(events, ProviderEvent::Continuation(id)).await?;
                                }
                                NormalizedEvent::Activity(message) => {
                                    send_event(events, ProviderEvent::Activity(ProviderActivity {
                                        title: message.content,
                                        detail: None,
                                        kind: message.kind,
                                    })).await?;
                                }
                            }
                        }
                    }
                    ProcessOutput::Exited(status) => break status,
                }
            }
        }
    };
    if status.success() {
        Ok(())
    } else {
        let detail = process.stderr_tail();
        Err(if detail.is_empty() {
            format!("{} exited with {status}", provider.display_name())
        } else {
            format!("{} exited with {status}: {detail}", provider.display_name())
        })
    }
}

async fn send_event(
    sender: &mpsc::Sender<ProviderEvent>,
    event: ProviderEvent,
) -> Result<(), String> {
    sender
        .send(event)
        .await
        .map_err(|_| "Provider event receiver closed".to_string())
}

fn build_args(
    provider: ProviderId,
    provider_session_id: Option<&str>,
    model: Option<&str>,
    prompt: &str,
) -> Vec<String> {
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

#[cfg(test)]
mod tests {
    use super::build_args;
    use crate::model::ProviderId;

    #[test]
    fn codex_fallback_resume_uses_documented_subcommand() {
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
}
