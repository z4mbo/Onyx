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
        steer: mpsc::UnboundedReceiver<String>,
    ) -> DriverFuture<'a, Result<(), String>> {
        // One-shot CLI transports cannot accept mid-turn input.
        drop(steer);
        Box::pin(async move {
            if let Some(detail) = self.startup_activity.take() {
                send_event(
                    &events,
                    ProviderEvent::Activity(ProviderActivity {
                        id: None,
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
    let args = build_args(config, continuation, prompt);
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
                                        id: None,
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

pub async fn chat_once(
    provider: ProviderId,
    model: Option<String>,
    prompt: String,
    web_search: bool,
) -> Result<String, String> {
    if provider == ProviderId::Openrouter {
        return Err("OpenRouter chat uses the HTTPS transport".into());
    }
    if prompt.trim().is_empty() || prompt.len() > 24 * 1024 {
        return Err("Chat prompt is empty or exceeds 24 KiB".into());
    }
    let workspace = std::env::temp_dir().join("onyx-chat-runtime");
    tokio::fs::create_dir_all(&workspace)
        .await
        .map_err(|error| error.to_string())?;
    let config = ProviderSessionConfig {
        provider,
        model,
        workspace,
        continuation: None,
        reasoning: Some(crate::model::ReasoningEffort::Medium),
        speed_mode: crate::model::SpeedMode::Standard,
        interaction_mode: crate::model::InteractionMode::Build,
        access_mode: crate::model::AccessMode::ApprovalRequired,
    };
    let capability_guard = if web_search {
        "You may use your read-only web-search capability when it improves the answer. Do not inspect local files, run mutation commands, or modify the computer."
    } else {
        "Do not inspect files, run commands, or modify the computer."
    };
    let guarded_prompt = format!(
        "You are Onyx Chat. Answer as a general conversational assistant. {capability_guard}\n\n{}",
        prompt.trim()
    );
    let (sender, mut receiver) = mpsc::channel(64);
    let cancellation = CancellationToken::new();
    let mut continuation = None;
    let mut turn = Box::pin(run_one_shot(
        &config,
        None,
        &guarded_prompt,
        &cancellation,
        &sender,
        &mut continuation,
    ));
    let mut content = String::new();
    let mut events_open = true;
    let result = loop {
        tokio::select! {
            event = receiver.recv(), if events_open => match event {
                Some(ProviderEvent::TextDelta(delta)) | Some(ProviderEvent::Text(delta)) => {
                    if content.len().saturating_add(delta.len()) > 2 * 1024 * 1024 {
                        cancellation.cancel();
                        break Err("Chat response exceeded the 2 MiB limit".into());
                    }
                    content.push_str(&delta);
                }
                Some(_) => {}
                None => events_open = false,
            },
            result = &mut turn => break result,
        }
    };
    drop(turn);
    result?;
    let content = content.trim().to_string();
    if content.is_empty() {
        Err(format!(
            "{} returned an empty chat response",
            provider.display_name()
        ))
    } else {
        Ok(content)
    }
}

fn build_args(
    config: &ProviderSessionConfig,
    provider_session_id: Option<&str>,
    prompt: &str,
) -> Vec<String> {
    let provider = config.provider;
    let model = config.model();
    // Keep the user's prompt byte-for-byte intact. Provider-native mode flags
    // carry planning and permission semantics just as they do in the CLI.
    let effective_prompt = prompt.to_string();
    match provider {
        ProviderId::Claude => {
            let mut args = vec![
                "--print".to_string(),
                "--verbose".to_string(),
                "--output-format".to_string(),
                "stream-json".to_string(),
                "--include-partial-messages".to_string(),
                "--permission-mode".to_string(),
                match (config.interaction_mode, config.access_mode) {
                    (crate::model::InteractionMode::Plan, _) => "plan",
                    (_, crate::model::AccessMode::ApprovalRequired) => "manual",
                    (_, crate::model::AccessMode::AutoAcceptEdits) => "acceptEdits",
                    (_, crate::model::AccessMode::FullAccess) => "bypassPermissions",
                }
                .to_string(),
            ];
            if config.access_mode == crate::model::AccessMode::FullAccess
                && config.interaction_mode != crate::model::InteractionMode::Plan
            {
                args.push("--dangerously-skip-permissions".to_string());
            }
            if let Some(reasoning) = config.reasoning {
                args.extend(["--effort".to_string(), reasoning.clamped_str().to_string()]);
            }
            let mut settings = serde_json::Map::new();
            if config.speed_mode == crate::model::SpeedMode::Fast {
                settings.insert("fastMode".into(), serde_json::Value::Bool(true));
            }
            if config.reasoning == Some(crate::model::ReasoningEffort::Ultracode) {
                settings.insert("ultracode".into(), serde_json::Value::Bool(true));
            }
            if !settings.is_empty() {
                args.extend([
                    "--settings".to_string(),
                    serde_json::Value::Object(settings).to_string(),
                ]);
            }
            if let Some(id) = provider_session_id {
                args.extend(["--resume".to_string(), id.to_string()]);
            }
            if let Some(model) = model {
                args.extend(["--model".to_string(), model.to_string()]);
            }
            args.push(effective_prompt);
            args
        }
        ProviderId::Codex => {
            let mut args = vec!["exec".to_string()];
            if let Some(id) = provider_session_id {
                args.extend([
                    "--sandbox".to_string(),
                    config.sandbox_name().to_string(),
                    "resume".to_string(),
                    "--json".to_string(),
                    "--skip-git-repo-check".to_string(),
                ]);
                if let Some(model) = model {
                    args.extend(["--model".to_string(), model.to_string()]);
                }
                args.extend([
                    "-c".to_string(),
                    format!("approval_policy=\"{}\"", config.approval_policy()),
                ]);
                if let Some(reasoning) = config.reasoning {
                    args.extend([
                        "-c".to_string(),
                        format!("model_reasoning_effort=\"{}\"", reasoning.clamped_str()),
                    ]);
                }
                if let Some(tier) = config.speed_mode.as_service_tier() {
                    args.extend(["-c".to_string(), format!("service_tier=\"{tier}\"")]);
                }
                args.extend([id.to_string(), effective_prompt]);
            } else {
                args.extend([
                    "--json".to_string(),
                    "--sandbox".to_string(),
                    config.sandbox_name().to_string(),
                    "--skip-git-repo-check".to_string(),
                ]);
                args.extend([
                    "-c".to_string(),
                    format!("approval_policy=\"{}\"", config.approval_policy()),
                ]);
                if let Some(reasoning) = config.reasoning {
                    args.extend([
                        "-c".to_string(),
                        format!("model_reasoning_effort=\"{}\"", reasoning.clamped_str()),
                    ]);
                }
                if let Some(tier) = config.speed_mode.as_service_tier() {
                    args.extend(["-c".to_string(), format!("service_tier=\"{tier}\"")]);
                }
                if let Some(model) = model {
                    args.extend(["--model".to_string(), model.to_string()]);
                }
                args.push(effective_prompt);
            }
            args
        }
        ProviderId::Gemini => {
            let mut args = vec![
                "--prompt".to_string(),
                effective_prompt,
                "--output-format".to_string(),
                "stream-json".to_string(),
                "--approval-mode".to_string(),
                match (config.interaction_mode, config.access_mode) {
                    (crate::model::InteractionMode::Plan, _) => "plan",
                    (_, crate::model::AccessMode::ApprovalRequired) => "default",
                    (_, crate::model::AccessMode::AutoAcceptEdits) => "auto_edit",
                    (_, crate::model::AccessMode::FullAccess) => "yolo",
                }
                .to_string(),
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
                effective_prompt,
                "--output-format".to_string(),
                "stream-json".to_string(),
            ];
            match config.access_mode {
                crate::model::AccessMode::ApprovalRequired => {}
                crate::model::AccessMode::AutoAcceptEdits => args.push("--yolo".to_string()),
                crate::model::AccessMode::FullAccess => args.push("--auto".to_string()),
            }
            if config.interaction_mode == crate::model::InteractionMode::Plan {
                args.push("--plan".to_string());
            }
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
    use crate::{
        model::{AccessMode, InteractionMode, ProviderId, SpeedMode},
        providers::driver::ProviderSessionConfig,
    };
    use std::path::PathBuf;

    fn config(provider: ProviderId) -> ProviderSessionConfig {
        ProviderSessionConfig {
            provider,
            model: None,
            workspace: PathBuf::from("/tmp/project"),
            continuation: None,
            reasoning: None,
            speed_mode: SpeedMode::Standard,
            interaction_mode: InteractionMode::Build,
            access_mode: AccessMode::AutoAcceptEdits,
        }
    }

    #[test]
    fn codex_fallback_resume_uses_documented_subcommand() {
        assert_eq!(
            build_args(&config(ProviderId::Codex), Some("thread"), "continue"),
            [
                "exec",
                "--sandbox",
                "workspace-write",
                "resume",
                "--json",
                "--skip-git-repo-check",
                "-c",
                "approval_policy=\"on-request\"",
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
            let args = build_args(&config(provider), None, "hello; rm -rf ignored");
            assert!(args.iter().any(|arg| arg == "hello; rm -rf ignored"));
        }
    }

    #[test]
    fn gemini_headless_mode_allows_edits() {
        let args = build_args(&config(ProviderId::Gemini), None, "fix it");
        assert!(
            args.windows(2)
                .any(|pair| pair == ["--approval-mode", "auto_edit"])
        );
    }

    #[test]
    fn gemini_maps_plan_permissions_model_and_resume() {
        let mut selected = config(ProviderId::Gemini);
        selected.model = Some("gemini-pro".to_string());
        selected.interaction_mode = InteractionMode::Plan;
        selected.access_mode = AccessMode::FullAccess;
        let args = build_args(&selected, Some("session-1"), "inspect");
        assert!(
            args.windows(2)
                .any(|pair| pair == ["--approval-mode", "plan"])
        );
        assert!(
            args.windows(2)
                .any(|pair| pair == ["--resume", "session-1"])
        );
        assert!(
            args.windows(2)
                .any(|pair| pair == ["--model", "gemini-pro"])
        );
        assert!(args.iter().any(|arg| arg == "inspect"));
    }

    #[test]
    fn kimi_maps_plan_permissions_model_and_resume() {
        let mut selected = config(ProviderId::Kimi);
        selected.model = Some("kimi-latest".to_string());
        selected.interaction_mode = InteractionMode::Plan;
        selected.access_mode = AccessMode::FullAccess;
        let args = build_args(&selected, Some("session-1"), "inspect");
        assert!(args.iter().any(|arg| arg == "--auto"));
        assert!(args.iter().any(|arg| arg == "--plan"));
        assert!(
            args.windows(2)
                .any(|pair| pair == ["--session", "session-1"])
        );
        assert!(
            args.windows(2)
                .any(|pair| pair == ["--model", "kimi-latest"])
        );
    }
}
