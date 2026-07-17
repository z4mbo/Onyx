use std::{ffi::OsString, path::PathBuf, process::Stdio, time::Duration};

use serde::Serialize;
use serde_json::{Value, json};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader, Lines},
    process::{Child, ChildStdin, ChildStdout, Command},
    sync::Mutex,
    time::timeout,
};

use crate::models::{ModelOption, SearchReply, SearchRequest, SearchSource, SearchUsage};

const RPC_TIMEOUT: Duration = Duration::from_secs(20);
const TURN_TIMEOUT: Duration = Duration::from_secs(180);
#[cfg(target_os = "windows")]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;
// Codex app-server deliberately exposes two different wire formats here:
// thread/start uses the legacy kebab-case enum, while turn/start uses the
// structured v2 policy discriminator.
const THREAD_SANDBOX_READ_ONLY: &str = "read-only";
const TURN_SANDBOX_READ_ONLY: &str = "readOnly";

#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct CodexAccountStatus {
    pub available: bool,
    pub connected: bool,
    pub auth_mode: Option<String>,
    pub email: Option<String>,
    pub plan_type: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CodexLoginStart {
    pub login_id: String,
    pub auth_url: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CodexDeviceLoginStart {
    pub login_id: String,
    pub verification_url: String,
    pub user_code: String,
}

#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct CodexRateLimits {
    pub primary_used_percent: Option<f64>,
    pub primary_window_minutes: Option<u64>,
    pub primary_resets_at: Option<u64>,
    pub secondary_used_percent: Option<f64>,
    pub secondary_window_minutes: Option<u64>,
    pub secondary_resets_at: Option<u64>,
}

/// Owns a single official `codex app-server` child. Onyx never reads or stores
/// ChatGPT tokens: the Codex runtime owns OAuth, persistence and refresh.
pub struct CodexState {
    inner: Mutex<Option<CodexSession>>,
}

impl Default for CodexState {
    fn default() -> Self {
        Self {
            inner: Mutex::new(None),
        }
    }
}

impl CodexState {
    pub async fn account_status(&self) -> Result<CodexAccountStatus, String> {
        let result = self
            .request("account/read", json!({ "refreshToken": false }))
            .await?;
        let account = result.get("account");
        let account_type = account
            .and_then(|value| value.get("type"))
            .and_then(Value::as_str)
            .map(str::to_owned);
        Ok(CodexAccountStatus {
            available: true,
            connected: account_type.as_deref() == Some("chatgpt"),
            auth_mode: account_type,
            email: account
                .and_then(|value| value.get("email"))
                .and_then(Value::as_str)
                .map(str::to_owned),
            plan_type: account
                .and_then(|value| value.get("planType"))
                .and_then(Value::as_str)
                .map(str::to_owned),
        })
    }

    pub async fn begin_login(&self) -> Result<CodexLoginStart, String> {
        let result = self
            .request(
                "account/login/start",
                json!({
                    "type": "chatgpt",
                    "useHostedLoginSuccessPage": true,
                    "appBrand": "chatgpt"
                }),
            )
            .await?;
        let login_id = result
            .get("loginId")
            .and_then(Value::as_str)
            .ok_or_else(|| "Codex non ha restituito un identificativo di login.".to_string())?;
        let auth_url = result
            .get("authUrl")
            .and_then(Value::as_str)
            .ok_or_else(|| "Codex non ha restituito l'indirizzo di autorizzazione.".to_string())?;
        validate_auth_url(auth_url)?;
        Ok(CodexLoginStart {
            login_id: login_id.to_owned(),
            auth_url: auth_url.to_owned(),
        })
    }

    pub async fn begin_device_login(&self) -> Result<CodexDeviceLoginStart, String> {
        let result = self
            .request(
                "account/login/start",
                json!({ "type": "chatgptDeviceCode" }),
            )
            .await?;
        let login_id = result
            .get("loginId")
            .and_then(Value::as_str)
            .ok_or_else(|| "Codex non ha restituito un identificativo di login.".to_string())?;
        let verification_url = result
            .get("verificationUrl")
            .and_then(Value::as_str)
            .ok_or_else(|| {
                "Codex non ha restituito l'indirizzo per il device login.".to_string()
            })?;
        let user_code = result
            .get("userCode")
            .and_then(Value::as_str)
            .ok_or_else(|| "Codex non ha restituito il codice utente.".to_string())?;
        validate_auth_url(verification_url)?;
        Ok(CodexDeviceLoginStart {
            login_id: login_id.to_owned(),
            verification_url: verification_url.to_owned(),
            user_code: user_code.to_owned(),
        })
    }

    pub async fn logout(&self) -> Result<(), String> {
        self.request("account/logout", json!({})).await.map(|_| ())
    }

    pub async fn list_models(&self) -> Result<Vec<ModelOption>, String> {
        let result = self
            .request(
                "model/list",
                json!({ "limit": 100, "includeHidden": false }),
            )
            .await?;
        let rows = result
            .get("data")
            .or_else(|| result.get("models"))
            .and_then(Value::as_array)
            .ok_or_else(|| {
                "Il catalogo modelli Codex ha restituito un formato inatteso.".to_string()
            })?;
        let mut models = Vec::new();
        for row in rows {
            let Some(id) = row.get("id").and_then(Value::as_str) else {
                continue;
            };
            if row.get("hidden").and_then(Value::as_bool).unwrap_or(false) {
                continue;
            }
            let name = row
                .get("displayName")
                .or_else(|| row.get("name"))
                .and_then(Value::as_str)
                .unwrap_or(id);
            let efforts = row
                .get("supportedReasoningEfforts")
                .and_then(Value::as_array)
                .map(|items| {
                    items
                        .iter()
                        .filter_map(|item| {
                            item.as_str()
                                .or_else(|| item.get("reasoningEffort").and_then(Value::as_str))
                        })
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            let description = row
                .get("description")
                .and_then(Value::as_str)
                .map(str::to_owned)
                .or_else(|| {
                    (!efforts.is_empty()).then(|| format!("Reasoning: {}", efforts.join(", ")))
                });
            models.push(ModelOption {
                id: id.to_owned(),
                name: name.to_owned(),
                description,
                prompt_price: None,
                completion_price: None,
            });
        }
        if models.is_empty() {
            return Err("Nessun modello disponibile per questo account ChatGPT/Codex.".into());
        }
        Ok(models)
    }

    pub async fn rate_limits(&self) -> Result<CodexRateLimits, String> {
        let result = self.request("account/rateLimits/read", json!({})).await?;
        let limits = result.get("rateLimits").unwrap_or(&result);
        Ok(CodexRateLimits {
            primary_used_percent: number_at(limits, &["primary", "usedPercent"]),
            primary_window_minutes: integer_at(limits, &["primary", "windowDurationMins"]),
            primary_resets_at: integer_at(limits, &["primary", "resetsAt"]),
            secondary_used_percent: number_at(limits, &["secondary", "usedPercent"]),
            secondary_window_minutes: integer_at(limits, &["secondary", "windowDurationMins"]),
            secondary_resets_at: integer_at(limits, &["secondary", "resetsAt"]),
        })
    }

    pub async fn search(&self, request: &SearchRequest) -> Result<SearchReply, String> {
        if request.query.trim().is_empty() || request.query.len() > 20_000 {
            return Err("La richiesta è vuota o troppo lunga.".into());
        }

        let mut guard = self.inner.lock().await;
        let session = ensure_session(&mut guard).await?;
        let model = request.model.trim();
        let mut thread_params = json!({
            "approvalPolicy": "never",
            "sandbox": THREAD_SANDBOX_READ_ONLY,
            "ephemeral": true,
            "personality": "friendly",
            "serviceName": "onyx",
            "developerInstructions": "Sei Onyx, un assistente di ricerca vocale. Rispondi nella lingua dell'utente. Puoi usare esclusivamente la ricerca web integrata quando serve. Non eseguire comandi, non leggere file locali, non modificare file, non controllare applicazioni e non usare strumenti diversi dalla ricerca web. Fornisci una risposta chiara e cita fonti verificabili con link."
        });
        let isolated_cwd = std::env::temp_dir().join("OnyxCodex");
        std::fs::create_dir_all(&isolated_cwd)
            .map_err(|error| format!("Non riesco a preparare l'ambiente isolato Codex: {error}"))?;
        thread_params["cwd"] = Value::String(isolated_cwd.to_string_lossy().to_string());
        if !model.is_empty() && model != "codex/default" {
            thread_params["model"] = Value::String(model.to_owned());
        }

        let thread_result = match session
            .request("thread/start", thread_params, RPC_TIMEOUT)
            .await
        {
            Ok(value) => value,
            Err(error) => {
                *guard = None;
                return Err(error);
            }
        };
        let thread_id = thread_result
            .pointer("/thread/id")
            .and_then(Value::as_str)
            .ok_or_else(|| {
                "Codex non ha restituito l'identificativo della conversazione.".to_string()
            })?
            .to_owned();

        let prompt = format!(
            "Rispondi a questa richiesta come assistente di ricerca. Usa il web se la risposta dipende da informazioni correnti e includi le fonti consultate. Non accedere al computer locale.\n\nRichiesta: {}",
            request.query.trim()
        );
        let mut turn_params = json!({
            "threadId": thread_id,
            "input": [{ "type": "text", "text": prompt }],
            "approvalPolicy": "never",
            "sandboxPolicy": {
                "type": TURN_SANDBOX_READ_ONLY,
                "networkAccess": false
            },
            "effort": normalize_effort(request.reasoning.as_deref()),
            "summary": "concise",
            "personality": "friendly"
        });
        if !model.is_empty() && model != "codex/default" {
            turn_params["model"] = Value::String(model.to_owned());
        }
        let turn_result = match session
            .request("turn/start", turn_params, RPC_TIMEOUT)
            .await
        {
            Ok(value) => value,
            Err(error) => {
                *guard = None;
                return Err(error);
            }
        };
        let turn_id = turn_result
            .pointer("/turn/id")
            .and_then(Value::as_str)
            .ok_or_else(|| "Codex non ha restituito l'identificativo della risposta.".to_string())?
            .to_owned();

        let collected = match session.collect_turn(&thread_id, &turn_id).await {
            Ok(value) => value,
            Err(error) => {
                *guard = None;
                return Err(error);
            }
        };
        Ok(SearchReply {
            answer: collected.answer,
            model: if collected.model.is_empty() {
                if model.is_empty() {
                    "Codex · automatico".into()
                } else {
                    model.to_owned()
                }
            } else {
                collected.model
            },
            sources: collected.sources,
            usage: collected.usage,
        })
    }

    async fn request(&self, method: &str, params: Value) -> Result<Value, String> {
        let mut guard = self.inner.lock().await;
        let session = ensure_session(&mut guard).await?;
        match session.request(method, params, RPC_TIMEOUT).await {
            Ok(value) => Ok(value),
            Err(error) => {
                *guard = None;
                Err(error)
            }
        }
    }
}

struct CollectedTurn {
    answer: String,
    model: String,
    sources: Vec<SearchSource>,
    usage: SearchUsage,
}

struct CodexSession {
    _child: Child,
    stdin: ChildStdin,
    stdout: Lines<BufReader<ChildStdout>>,
    next_id: u64,
}

impl CodexSession {
    async fn start() -> Result<Self, String> {
        let candidates = codex_candidates();
        let mut failures = Vec::new();
        for candidate in candidates {
            match Self::start_candidate(&candidate).await {
                Ok(session) => return Ok(session),
                Err(error) => failures.push(format!("{}: {error}", candidate.to_string_lossy())),
            }
        }
        Err(format!(
            "Runtime Codex non disponibile. Installa o aggiorna l'app/CLI ufficiale Codex, oppure imposta ONYX_CODEX_BIN. Dettagli: {}",
            failures.join(" | ")
        ))
    }

    async fn start_candidate(candidate: &OsString) -> Result<Self, String> {
        let mut command = Command::new(candidate);
        command
            .arg("app-server")
            .arg("--listen")
            .arg("stdio://")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .kill_on_drop(true);
        #[cfg(target_os = "windows")]
        command.creation_flags(CREATE_NO_WINDOW);
        let mut child = command.spawn().map_err(|error| error.to_string())?;
        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| "stdin Codex non disponibile.".to_string())?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| "stdout Codex non disponibile.".to_string())?;
        let mut session = Self {
            _child: child,
            stdin,
            stdout: BufReader::new(stdout).lines(),
            next_id: 1,
        };
        session.request(
            "initialize",
            json!({
                "clientInfo": { "name": "onyx", "title": "Onyx", "version": env!("CARGO_PKG_VERSION") },
                "capabilities": {}
            }),
            RPC_TIMEOUT,
        ).await?;
        session
            .write(json!({ "method": "initialized", "params": {} }))
            .await?;
        Ok(session)
    }

    async fn request(
        &mut self,
        method: &str,
        params: Value,
        wait: Duration,
    ) -> Result<Value, String> {
        let id = self.next_id;
        self.next_id = self.next_id.saturating_add(1);
        self.write(json!({ "method": method, "id": id, "params": params }))
            .await?;
        loop {
            let message = self.read(wait).await?;
            if message.get("id").and_then(Value::as_u64) == Some(id) {
                if let Some(error) = message.get("error") {
                    return Err(rpc_error(error));
                }
                return Ok(message.get("result").cloned().unwrap_or(Value::Null));
            }
            self.decline_server_request(&message).await?;
        }
    }

    async fn collect_turn(
        &mut self,
        thread_id: &str,
        turn_id: &str,
    ) -> Result<CollectedTurn, String> {
        let mut answer = String::new();
        let mut completed_answer = String::new();
        let mut model = String::new();
        let mut sources = Vec::new();
        let mut usage = SearchUsage::default();
        loop {
            let message = self.read(TURN_TIMEOUT).await?;
            self.decline_server_request(&message).await?;
            let method = message
                .get("method")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let params = message.get("params").unwrap_or(&Value::Null);
            if let Some(rerouted) = (method == "model/rerouted").then_some(params) {
                if rerouted.get("threadId").and_then(Value::as_str) == Some(thread_id) {
                    model = rerouted
                        .get("toModel")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_owned();
                }
            }
            if method == "item/agentMessage/delta"
                && params.get("threadId").and_then(Value::as_str) == Some(thread_id)
                && params.get("turnId").and_then(Value::as_str) == Some(turn_id)
            {
                if let Some(delta) = params.get("delta").and_then(Value::as_str) {
                    answer.push_str(delta);
                }
            }
            if method == "item/completed"
                && params.get("threadId").and_then(Value::as_str) == Some(thread_id)
                && params.get("turnId").and_then(Value::as_str) == Some(turn_id)
            {
                let item = params.get("item").unwrap_or(&Value::Null);
                match item.get("type").and_then(Value::as_str) {
                    Some("agentMessage") => {
                        if let Some(text) = item.get("text").and_then(Value::as_str) {
                            completed_answer = text.to_owned();
                        }
                    }
                    Some("webSearch") => append_sources(item, &mut sources),
                    _ => {}
                }
            }
            if method == "item/started"
                && params.get("threadId").and_then(Value::as_str) == Some(thread_id)
                && params.get("turnId").and_then(Value::as_str) == Some(turn_id)
            {
                let item_type = params
                    .pointer("/item/type")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                if matches!(
                    item_type,
                    "commandExecution"
                        | "fileChange"
                        | "mcpToolCall"
                        | "dynamicToolCall"
                        | "collabAgentToolCall"
                ) {
                    let _ = self
                        .request(
                            "turn/interrupt",
                            json!({ "threadId": thread_id, "turnId": turn_id }),
                            RPC_TIMEOUT,
                        )
                        .await;
                    return Err(format!(
                        "Codex ha richiesto lo strumento non consentito “{item_type}”. Onyx ha interrotto il turno: in questa versione è permessa soltanto la ricerca web."
                    ));
                }
            }
            if method == "thread/tokenUsage/updated"
                && params.get("threadId").and_then(Value::as_str) == Some(thread_id)
            {
                update_usage(params, &mut usage);
            }
            if method == "turn/completed" {
                let turn = params.get("turn").unwrap_or(&Value::Null);
                if turn.get("id").and_then(Value::as_str) != Some(turn_id) {
                    continue;
                }
                match turn
                    .get("status")
                    .and_then(Value::as_str)
                    .unwrap_or("failed")
                {
                    "completed" => {
                        let final_answer = if completed_answer.trim().is_empty() {
                            answer
                        } else {
                            completed_answer
                        };
                        if final_answer.trim().is_empty() {
                            return Err(
                                "Codex ha completato il turno senza restituire testo.".into()
                            );
                        }
                        append_markdown_sources(&final_answer, &mut sources);
                        deduplicate_sources(&mut sources);
                        return Ok(CollectedTurn {
                            answer: final_answer,
                            model,
                            sources,
                            usage,
                        });
                    }
                    "interrupted" => return Err("La risposta Codex è stata interrotta.".into()),
                    _ => {
                        let error = turn
                            .pointer("/error/message")
                            .and_then(Value::as_str)
                            .unwrap_or("Il turno Codex non è riuscito.");
                        return Err(error.to_owned());
                    }
                }
            }
        }
    }

    async fn write(&mut self, message: Value) -> Result<(), String> {
        let mut encoded = serde_json::to_vec(&message).map_err(|error| error.to_string())?;
        encoded.push(b'\n');
        self.stdin
            .write_all(&encoded)
            .await
            .map_err(|error| format!("Scrittura verso Codex fallita: {error}"))?;
        self.stdin
            .flush()
            .await
            .map_err(|error| format!("Flush verso Codex fallito: {error}"))
    }

    async fn read(&mut self, wait: Duration) -> Result<Value, String> {
        let line = timeout(wait, self.stdout.next_line())
            .await
            .map_err(|_| "Codex non ha risposto entro il tempo previsto.".to_string())?
            .map_err(|error| format!("Lettura da Codex fallita: {error}"))?
            .ok_or_else(|| "Il runtime Codex si è chiuso inaspettatamente.".to_string())?;
        serde_json::from_str(&line).map_err(|error| format!("Messaggio Codex non valido: {error}"))
    }

    async fn decline_server_request(&mut self, message: &Value) -> Result<(), String> {
        if message.get("method").is_none() || message.get("id").is_none() {
            return Ok(());
        }
        let id = message.get("id").cloned().unwrap_or(Value::Null);
        self.write(json!({ "id": id, "result": { "decision": "decline" } }))
            .await
    }
}

async fn ensure_session(guard: &mut Option<CodexSession>) -> Result<&mut CodexSession, String> {
    if guard.is_none() {
        *guard = Some(CodexSession::start().await?);
    }
    guard
        .as_mut()
        .ok_or_else(|| "Runtime Codex non disponibile.".into())
}

fn codex_candidates() -> Vec<OsString> {
    let mut candidates = Vec::new();
    if let Some(configured) = std::env::var_os("ONYX_CODEX_BIN") {
        candidates.push(configured);
    }
    if let Ok(current) = std::env::current_exe() {
        if let Some(parent) = current.parent() {
            let adjacent = parent.join(if cfg!(target_os = "windows") {
                "codex.exe"
            } else {
                "codex"
            });
            if adjacent.is_file() {
                candidates.push(adjacent.into_os_string());
            }
        }
    }
    #[cfg(target_os = "windows")]
    if let Some(local) = std::env::var_os("LOCALAPPDATA") {
        let local = PathBuf::from(local);
        let desktop_runtimes = local.join("OpenAI").join("Codex").join("bin");
        if let Ok(entries) = std::fs::read_dir(desktop_runtimes) {
            let mut installed = entries
                .filter_map(Result::ok)
                .filter_map(|entry| {
                    let executable = entry.path().join("codex.exe");
                    let modified = executable.metadata().ok()?.modified().ok()?;
                    executable
                        .is_file()
                        .then_some((modified, executable.into_os_string()))
                })
                .collect::<Vec<_>>();
            installed.sort_by(|left, right| right.0.cmp(&left.0));
            candidates.extend(installed.into_iter().map(|(_, executable)| executable));
        }
        let alias = local
            .join("Microsoft")
            .join("WindowsApps")
            .join("codex.exe");
        if alias.is_file() {
            candidates.push(alias.into_os_string());
        }
    }
    #[cfg(target_os = "macos")]
    {
        // GUI apps launched from Finder do not inherit the user's shell PATH.
        // Check the official Codex Desktop bundle and the common CLI install
        // locations explicitly before falling back to PATH lookup.
        for executable in [
            PathBuf::from("/Applications/Codex.app/Contents/Resources/codex"),
            PathBuf::from("/opt/homebrew/bin/codex"),
            PathBuf::from("/usr/local/bin/codex"),
        ] {
            if executable.is_file() {
                candidates.push(executable.into_os_string());
            }
        }
        if let Some(home) = std::env::var_os("HOME").map(PathBuf::from) {
            for executable in [
                home.join("Applications/Codex.app/Contents/Resources/codex"),
                home.join(".local/bin/codex"),
                home.join(".cargo/bin/codex"),
            ] {
                if executable.is_file() {
                    candidates.push(executable.into_os_string());
                }
            }
        }
    }
    candidates.push(OsString::from(if cfg!(target_os = "windows") {
        "codex.exe"
    } else {
        "codex"
    }));
    candidates.dedup();
    candidates
}

fn validate_auth_url(value: &str) -> Result<(), String> {
    let url = url::Url::parse(value).map_err(|_| "URL OAuth Codex non valido.".to_string())?;
    if url.scheme() != "https" {
        return Err("Codex ha restituito un URL OAuth non sicuro.".into());
    }
    let host = url.host_str().unwrap_or_default();
    if !matches!(host, "chatgpt.com" | "auth.openai.com") && !host.ends_with(".openai.com") {
        return Err("Codex ha restituito un dominio OAuth inatteso.".into());
    }
    Ok(())
}

fn normalize_effort(value: Option<&str>) -> &'static str {
    match value {
        Some("none") => "none",
        Some("low") => "low",
        Some("high") => "high",
        Some("xhigh") => "xhigh",
        _ => "medium",
    }
}

fn rpc_error(error: &Value) -> String {
    error
        .get("message")
        .and_then(Value::as_str)
        .unwrap_or("Richiesta Codex non riuscita.")
        .to_owned()
}

fn append_sources(item: &Value, output: &mut Vec<SearchSource>) {
    if let Some(url) = item.pointer("/action/url").and_then(Value::as_str) {
        push_source_url(url, None, Some("Pagina consultata da Codex".into()), output);
    }
    if let Some(results) = item.get("results").and_then(Value::as_array) {
        for result in results {
            let Some(url) = find_string(result, &["url", "link"]) else {
                continue;
            };
            let title = find_string(result, &["title", "name"]);
            let snippet = find_string(result, &["snippet", "description", "text"]);
            push_source_url(&url, title, snippet, output);
        }
    }
}

fn append_markdown_sources(answer: &str, output: &mut Vec<SearchSource>) {
    let mut rest = answer;
    while let Some(label_end) = rest.find("](") {
        let before = &rest[..label_end];
        let label_start = before.rfind('[').map(|index| index + 1).unwrap_or(0);
        let label = before[label_start..].trim();
        let after = &rest[label_end + 2..];
        let Some(url_end) = after.find(')') else {
            break;
        };
        let url = after[..url_end].trim();
        push_source_url(
            url,
            (!label.is_empty()).then(|| label.to_owned()),
            Some("Fonte citata nella risposta".into()),
            output,
        );
        rest = &after[url_end + 1..];
    }
}

fn push_source_url(
    value: &str,
    title: Option<String>,
    snippet: Option<String>,
    output: &mut Vec<SearchSource>,
) {
    let Ok(parsed) = url::Url::parse(value) else {
        return;
    };
    if !matches!(parsed.scheme(), "https" | "http") {
        return;
    }
    let url = parsed.to_string();
    let title = title.unwrap_or_else(|| parsed.host_str().unwrap_or(value).to_owned());
    output.push(SearchSource {
        title,
        url,
        snippet,
    });
}

fn find_string(value: &Value, keys: &[&str]) -> Option<String> {
    for key in keys {
        if let Some(found) = value.get(*key).and_then(Value::as_str) {
            return Some(found.to_owned());
        }
    }
    for child in value
        .as_object()
        .into_iter()
        .flat_map(|object| object.values())
    {
        if let Some(found) = find_string(child, keys) {
            return Some(found);
        }
    }
    None
}

fn deduplicate_sources(sources: &mut Vec<SearchSource>) {
    let mut seen = std::collections::HashSet::new();
    sources.retain(|source| seen.insert(source.url.clone()));
    sources.truncate(12);
}

fn update_usage(value: &Value, usage: &mut SearchUsage) {
    if let Some(input) = find_u64(value, &["inputTokens", "input_tokens"]) {
        usage.input_tokens = Some(input);
    }
    if let Some(output) = find_u64(value, &["outputTokens", "output_tokens"]) {
        usage.output_tokens = Some(output);
    }
}

fn find_u64(value: &Value, keys: &[&str]) -> Option<u64> {
    for key in keys {
        if let Some(found) = value.get(*key).and_then(Value::as_u64) {
            return Some(found);
        }
    }
    for child in value
        .as_object()
        .into_iter()
        .flat_map(|object| object.values())
    {
        if let Some(found) = find_u64(child, keys) {
            return Some(found);
        }
    }
    None
}

fn number_at(value: &Value, path: &[&str]) -> Option<f64> {
    path.iter()
        .try_fold(value, |current, key| current.get(*key))
        .and_then(Value::as_f64)
}

fn integer_at(value: &Value, path: &[&str]) -> Option<u64> {
    path.iter()
        .try_fold(value, |current, key| current.get(*key))
        .and_then(Value::as_u64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_unexpected_oauth_hosts() {
        assert!(validate_auth_url("https://chatgpt.com/oauth/authorize").is_ok());
        assert!(validate_auth_url("https://auth.openai.com/codex/device").is_ok());
        assert!(validate_auth_url("http://chatgpt.com/oauth").is_err());
        assert!(validate_auth_url("https://example.com/oauth").is_err());
    }

    #[test]
    fn extracts_and_deduplicates_search_sources() {
        let item = json!({ "results": [
            { "title": "A", "url": "https://example.com/a", "snippet": "one" },
            { "name": "A again", "link": "https://example.com/a" },
            { "title": "Bad", "url": "file:///tmp/x" }
        ] });
        let mut sources = Vec::new();
        append_sources(&item, &mut sources);
        deduplicate_sources(&mut sources);
        assert_eq!(sources.len(), 1);
        assert_eq!(sources[0].title, "A");
    }

    #[test]
    fn uses_codex_sandbox_wire_values_for_each_rpc_shape() {
        let thread = json!({ "sandbox": THREAD_SANDBOX_READ_ONLY });
        let turn = json!({
            "sandboxPolicy": {
                "type": TURN_SANDBOX_READ_ONLY,
                "networkAccess": false
            }
        });

        assert_eq!(thread["sandbox"], "read-only");
        assert_eq!(turn["sandboxPolicy"]["type"], "readOnly");
        assert_eq!(turn["sandboxPolicy"]["networkAccess"], false);
        assert!(turn["sandboxPolicy"].get("access").is_none());
    }
}
