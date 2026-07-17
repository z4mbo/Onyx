use std::{
    fs::{self, OpenOptions},
    io::Write as _,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

use reqwest::Client;
use serde_json::{Value, json};

use crate::{
    models::{TtsConfig, TtsSpeakReply, TtsVoiceOption},
    provider, secrets,
};

const OPENAI_SPEECH_URL: &str = "https://api.openai.com/v1/audio/speech";
const OPENROUTER_SPEECH_URL: &str = "https://openrouter.ai/api/v1/audio/speech";
const MAX_TEXT_CHARACTERS: usize = 4_096;
const MAX_INSTRUCTIONS_CHARACTERS: usize = 1_000;
const MAX_AUDIO_RESPONSE_BYTES: u64 = 32 * 1024 * 1024;
const PREVIEW_TEXT: &str = "Ciao, sono Onyx. Come posso aiutarti?";

const OPENAI_VOICES: [(&str, &str); 13] = [
    ("alloy", "Alloy"),
    ("ash", "Ash"),
    ("ballad", "Ballad"),
    ("coral", "Coral"),
    ("echo", "Echo"),
    ("fable", "Fable"),
    ("onyx", "Onyx"),
    ("nova", "Nova"),
    ("sage", "Sage"),
    ("shimmer", "Shimmer"),
    ("verse", "Verse"),
    ("marin", "Marin"),
    ("cedar", "Cedar"),
];

const GROK_VOICES: [(&str, &str); 5] = [
    ("Eve", "Eve"),
    ("Ara", "Ara"),
    ("Rex", "Rex"),
    ("Sal", "Sal"),
    ("Leo", "Leo"),
];

pub fn load_config(path: &Path) -> Result<TtsConfig, String> {
    let raw = match fs::read_to_string(path) {
        Ok(raw) => raw,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(TtsConfig::default());
        }
        Err(error) => {
            return Err(format!(
                "Non riesco a leggere le impostazioni voce: {error}"
            ));
        }
    };
    let parsed = serde_json::from_str::<TtsConfig>(&raw)
        .map_err(|_| "Il file delle impostazioni voce non è valido.".to_string())?;
    normalize_config(parsed)
}

pub fn save_config(path: &Path, config: &TtsConfig) -> Result<(), String> {
    let parent = path
        .parent()
        .ok_or_else(|| "Percorso impostazioni voce non valido.".to_string())?;
    fs::create_dir_all(parent)
        .map_err(|error| format!("Non riesco a creare la cartella impostazioni: {error}"))?;
    let payload = serde_json::to_vec_pretty(config)
        .map_err(|_| "Non riesco a preparare le impostazioni voce.".to_string())?;
    let mut file = OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .open(path)
        .map_err(|error| format!("Non riesco a salvare le impostazioni voce: {error}"))?;
    file.write_all(&payload)
        .and_then(|_| file.sync_all())
        .map_err(|error| format!("Non riesco a salvare le impostazioni voce: {error}"))
}

pub fn normalize_config(mut config: TtsConfig) -> Result<TtsConfig, String> {
    config.provider = config.provider.trim().to_ascii_lowercase();
    config.model = config.model.trim().to_string();
    config.voice = config.voice.trim().to_string();
    config.instructions = config
        .instructions
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());
    config.system_voice = config
        .system_voice
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());

    if !matches!(config.provider.as_str(), "openai" | "openrouter" | "system") {
        return Err("Provider voce non supportato.".into());
    }
    if !config.speed.is_finite() || !(0.25..=4.0).contains(&config.speed) {
        return Err("La velocità della voce deve essere compresa tra 0.25× e 4×.".into());
    }
    validate_voice_id(&config.voice)?;
    if let Some(system_voice) = config.system_voice.as_deref() {
        validate_voice_id(system_voice)?;
    }
    if let Some(instructions) = config.instructions.as_deref()
        && (instructions.chars().count() > MAX_INSTRUCTIONS_CHARACTERS
            || instructions.contains('\0'))
    {
        return Err("Le istruzioni vocali sono troppo lunghe o non valide.".into());
    }

    match config.provider.as_str() {
        "openai" => {
            validate_openai_model(&config.model)?;
            if !OPENAI_VOICES.iter().any(|(id, _)| *id == config.voice) {
                return Err("Voce OpenAI non supportata.".into());
            }
        }
        "openrouter" => {
            provider::validate_model_id(&config.model)?;
            if !config.model.contains('/') {
                return Err("Il modello OpenRouter deve includere il provider, ad esempio openai/gpt-4o-mini-tts.".into());
            }
        }
        "system" => {}
        _ => unreachable!(),
    }
    Ok(config)
}

pub async fn list_voices(
    provider_name: &str,
    model: Option<&str>,
    configured_voice: &str,
) -> Result<Vec<TtsVoiceOption>, String> {
    match provider_name {
        "openai" => Ok(openai_voice_options("openai")),
        "openrouter" => {
            let model = model.unwrap_or_default();
            if model.starts_with("openai/") {
                Ok(openai_voice_options("openrouter"))
            } else if model.contains("grok-voice") {
                Ok(named_voice_options("openrouter", &GROK_VOICES))
            } else {
                // OpenRouter does not expose a provider-independent voice catalogue.
                // Preserve the configured model-specific identifier for dropdown UIs.
                validate_voice_id(configured_voice)?;
                Ok(vec![TtsVoiceOption {
                    id: configured_voice.into(),
                    name: configured_voice.into(),
                    provider: "openrouter".into(),
                    language: None,
                    local: false,
                }])
            }
        }
        "system" => tauri::async_runtime::spawn_blocking(list_system_voices_blocking)
            .await
            .map_err(|_| "Non riesco a caricare le voci di sistema.".to_string())?,
        _ => Err("Provider voce non supportato.".into()),
    }
}

pub fn preview_text(text: Option<String>) -> String {
    text.map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| PREVIEW_TEXT.into())
}

pub async fn speak(
    client: &Client,
    config: &TtsConfig,
    text: &str,
) -> Result<TtsSpeakReply, String> {
    let config = normalize_config(config.clone())?;
    let text = validate_text(text)?;
    let characters = text.chars().count();

    if config.provider == "system" {
        speak_system(text.to_string(), config.system_voice.clone(), config.speed).await?;
        return Ok(system_reply(&config, characters, false, None));
    }

    let cloud_result = speak_cloud(client, &config, text).await;
    match cloud_result {
        Ok(generation_id) => Ok(TtsSpeakReply {
            requested_provider: config.provider.clone(),
            provider: config.provider.clone(),
            model: Some(config.model.clone()),
            voice: config.voice.clone(),
            characters,
            used_fallback: false,
            generation_id,
            warning: None,
        }),
        Err(error) if config.fallback_to_system => {
            speak_system(text.to_string(), config.system_voice.clone(), config.speed).await?;
            Ok(system_reply(
                &config,
                characters,
                true,
                Some(format!("{error} È stata usata la voce di sistema.")),
            ))
        }
        Err(error) => Err(error),
    }
}

async fn speak_cloud(
    client: &Client,
    config: &TtsConfig,
    text: &str,
) -> Result<Option<String>, String> {
    let api_key = secrets::get_provider_key(&config.provider)?.ok_or_else(|| {
        format!(
            "Collega {} nella sezione Modelli prima di usare la voce cloud.",
            if config.provider == "openai" {
                "OpenAI"
            } else {
                "OpenRouter"
            }
        )
    })?;
    provider::validate_api_key_shape(&api_key)?;

    let response = match config.provider.as_str() {
        "openai" => client
            .post(OPENAI_SPEECH_URL)
            .bearer_auth(&api_key)
            .json(&openai_request_body(config, text))
            .send()
            .await
            .map_err(|error| provider::network_error("OpenAI", error))?,
        "openrouter" => client
            .post(OPENROUTER_SPEECH_URL)
            .bearer_auth(&api_key)
            .header("X-OpenRouter-Title", "Onyx")
            .json(&openrouter_request_body(config, text))
            .send()
            .await
            .map_err(|error| provider::network_error("OpenRouter", error))?,
        _ => return Err("Provider voce cloud non supportato.".into()),
    };
    let provider_name = if config.provider == "openai" {
        "OpenAI"
    } else {
        "OpenRouter"
    };
    let generation_id = response
        .headers()
        .get("x-generation-id")
        .or_else(|| response.headers().get("x-request-id"))
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned);
    let status = response.status();
    let announced_size = response.content_length();
    if announced_size.is_some_and(|size| size > MAX_AUDIO_RESPONSE_BYTES) {
        return Err(format!(
            "{provider_name} ha restituito un audio troppo grande."
        ));
    }
    let bytes = response
        .bytes()
        .await
        .map_err(|error| provider::network_error(provider_name, error))?;
    if !status.is_success() {
        let body = String::from_utf8_lossy(&bytes[..bytes.len().min(64 * 1024)]);
        return Err(provider::api_error(provider_name, status, &body));
    }
    if bytes.len() as u64 > MAX_AUDIO_RESPONSE_BYTES {
        return Err(format!(
            "{provider_name} ha restituito un audio troppo grande."
        ));
    }
    if bytes.is_empty() {
        return Err(format!("{provider_name} ha restituito un audio vuoto."));
    }
    play_cloud_audio(bytes.to_vec()).await?;
    Ok(generation_id)
}

fn openai_request_body(config: &TtsConfig, text: &str) -> Value {
    let mut body = json!({
        "model": config.model,
        "input": text,
        "voice": config.voice,
        "response_format": "mp3",
        "speed": config.speed,
    });
    if supports_instructions(&config.model)
        && let Some(instructions) = config.instructions.as_deref()
    {
        body["instructions"] = Value::String(instructions.into());
    }
    body
}

fn openrouter_request_body(config: &TtsConfig, text: &str) -> Value {
    let mut body = json!({
        "model": config.model,
        "input": text,
        "voice": config.voice,
        "response_format": "mp3",
        "speed": config.speed,
    });
    if config.model.starts_with("openai/")
        && let Some(instructions) = config.instructions.as_deref()
    {
        body["provider"] = json!({
            "options": {"openai": {"instructions": instructions}}
        });
    }
    body
}

fn supports_instructions(model: &str) -> bool {
    model.starts_with("gpt-4o-mini-tts")
}

fn validate_openai_model(model: &str) -> Result<(), String> {
    provider::validate_model_id(model)?;
    if matches!(model, "tts-1" | "tts-1-hd") || model.starts_with("gpt-4o-mini-tts") {
        Ok(())
    } else {
        Err("Modello OpenAI TTS non supportato.".into())
    }
}

fn validate_voice_id(voice: &str) -> Result<(), String> {
    let count = voice.chars().count();
    if count == 0 || count > 180 || voice.contains('\0') || voice.chars().any(char::is_control) {
        Err("Identificatore voce non valido.".into())
    } else {
        Ok(())
    }
}

fn validate_text(text: &str) -> Result<&str, String> {
    let text = text.trim();
    let length = text.chars().count();
    if length == 0 {
        return Err("Il testo da leggere è vuoto.".into());
    }
    if length > MAX_TEXT_CHARACTERS {
        return Err(format!(
            "Il testo da leggere supera il limite di {MAX_TEXT_CHARACTERS} caratteri."
        ));
    }
    if text.contains('\0') {
        return Err("Il testo da leggere non è valido.".into());
    }
    Ok(text)
}

fn openai_voice_options(provider_name: &str) -> Vec<TtsVoiceOption> {
    named_voice_options(provider_name, &OPENAI_VOICES)
}

fn named_voice_options(provider_name: &str, voices: &[(&str, &str)]) -> Vec<TtsVoiceOption> {
    voices
        .iter()
        .map(|(id, name)| TtsVoiceOption {
            id: (*id).into(),
            name: (*name).into(),
            provider: provider_name.into(),
            language: None,
            local: false,
        })
        .collect()
}

fn system_reply(
    config: &TtsConfig,
    characters: usize,
    used_fallback: bool,
    warning: Option<String>,
) -> TtsSpeakReply {
    TtsSpeakReply {
        requested_provider: config.provider.clone(),
        provider: "system".into(),
        model: None,
        voice: config
            .system_voice
            .clone()
            .unwrap_or_else(|| "default".into()),
        characters,
        used_fallback,
        generation_id: None,
        warning,
    }
}

async fn play_cloud_audio(audio: Vec<u8>) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        let temporary = TemporaryAudio::create(&audio)?;
        play_audio_file(temporary.path())
    })
    .await
    .map_err(|_| "Il player audio di sistema si è interrotto.".to_string())?
}

async fn speak_system(text: String, voice: Option<String>, speed: f32) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        speak_system_blocking(&text, voice.as_deref(), speed)
    })
    .await
    .map_err(|_| "La sintesi vocale di sistema si è interrotta.".to_string())?
}

struct TemporaryAudio {
    path: PathBuf,
}

impl TemporaryAudio {
    fn create(audio: &[u8]) -> Result<Self, String> {
        let mut random = [0_u8; 16];
        getrandom::fill(&mut random)
            .map_err(|_| "Non riesco a creare un file audio temporaneo sicuro.".to_string())?;
        let suffix = random
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        let path =
            std::env::temp_dir().join(format!("onyx-tts-{}-{suffix}.mp3", std::process::id()));
        let mut file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&path)
            .map_err(|error| format!("Non riesco a creare l'audio temporaneo: {error}"))?;
        if let Err(error) = file.write_all(audio).and_then(|_| file.sync_all()) {
            let _ = fs::remove_file(&path);
            return Err(format!("Non riesco a preparare l'audio: {error}"));
        }
        Ok(Self { path })
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TemporaryAudio {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

#[cfg(target_os = "windows")]
fn windows_powershell() -> PathBuf {
    std::env::var_os("SystemRoot")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(r"C:\Windows"))
        .join(r"System32\WindowsPowerShell\v1.0\powershell.exe")
}

#[cfg(target_os = "windows")]
fn hidden_command(program: &Path) -> Command {
    use std::os::windows::process::CommandExt as _;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let mut command = Command::new(program);
    command.creation_flags(CREATE_NO_WINDOW);
    command
}

#[cfg(target_os = "windows")]
fn list_system_voices_blocking() -> Result<Vec<TtsVoiceOption>, String> {
    const SCRIPT: &str = r#"
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Speech
$speaker = [System.Speech.Synthesis.SpeechSynthesizer]::new()
try {
  foreach ($voice in $speaker.GetInstalledVoices()) {
    if ($voice.Enabled) {
      $info = $voice.VoiceInfo
      [Console]::Out.WriteLine("{0}`t{1}`t{2}" -f $info.Name, $info.Culture.Name, $info.Gender)
    }
  }
} finally { $speaker.Dispose() }
"#;
    let output = hidden_command(&windows_powershell())
        .args([
            "-NoLogo",
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            SCRIPT,
        ])
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .map_err(|_| "La sintesi vocale Windows non è disponibile.".to_string())?;
    if !output.status.success() {
        return Err("La sintesi vocale Windows non è disponibile.".into());
    }
    let voices = parse_windows_voice_list(&String::from_utf8_lossy(&output.stdout));
    if voices.is_empty() {
        Err("Windows non ha restituito voci installate.".into())
    } else {
        Ok(voices)
    }
}

#[cfg(target_os = "windows")]
fn parse_windows_voice_list(output: &str) -> Vec<TtsVoiceOption> {
    output
        .lines()
        .filter_map(|line| {
            let mut fields = line.splitn(3, '\t');
            let id = fields.next()?.trim();
            let language = fields.next().unwrap_or_default().trim();
            let gender = fields.next().unwrap_or_default().trim();
            if id.is_empty() {
                return None;
            }
            Some(TtsVoiceOption {
                id: id.into(),
                name: if gender.is_empty() {
                    id.into()
                } else {
                    format!("{id} · {gender}")
                },
                provider: "system".into(),
                language: (!language.is_empty()).then(|| language.into()),
                local: true,
            })
        })
        .collect()
}

#[cfg(target_os = "windows")]
fn speak_system_blocking(text: &str, voice: Option<&str>, speed: f32) -> Result<(), String> {
    const SCRIPT: &str = r#"
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Speech
$speaker = [System.Speech.Synthesis.SpeechSynthesizer]::new()
try {
  $voice = [Environment]::GetEnvironmentVariable('ONYX_TTS_SYSTEM_VOICE')
  if (-not [string]::IsNullOrWhiteSpace($voice)) { $speaker.SelectVoice($voice) }
  $rate = [Environment]::GetEnvironmentVariable('ONYX_TTS_SYSTEM_RATE')
  if (-not [string]::IsNullOrWhiteSpace($rate)) { $speaker.Rate = [int]$rate }
  $text = [Console]::In.ReadToEnd()
  $speaker.Speak($text)
} finally { $speaker.Dispose() }
"#;
    let rate = (((speed - 1.0) * 5.0).round() as i32).clamp(-10, 10);
    let mut command = hidden_command(&windows_powershell());
    command
        .args([
            "-NoLogo",
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            SCRIPT,
        ])
        .env("ONYX_TTS_SYSTEM_RATE", rate.to_string())
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    if let Some(voice) = voice {
        command.env("ONYX_TTS_SYSTEM_VOICE", voice);
    } else {
        command.env_remove("ONYX_TTS_SYSTEM_VOICE");
    }
    let mut child = command
        .spawn()
        .map_err(|_| "La sintesi vocale Windows non è disponibile.".to_string())?;
    let Some(mut stdin) = child.stdin.take() else {
        let _ = child.kill();
        return Err("Non riesco a inviare il testo alla voce Windows.".into());
    };
    if stdin.write_all(text.as_bytes()).is_err() {
        let _ = child.kill();
        return Err("Non riesco a inviare il testo alla voce Windows.".into());
    }
    drop(stdin);
    let status = child
        .wait()
        .map_err(|_| "La sintesi vocale Windows si è interrotta.".to_string())?;
    if status.success() {
        Ok(())
    } else {
        Err("La voce Windows selezionata non è disponibile.".into())
    }
}

#[cfg(target_os = "windows")]
fn play_audio_file(path: &Path) -> Result<(), String> {
    let alias = "onyx_tts_audio";
    let open_command = format!("open \"{}\" type mpegvideo alias {alias}", path.display());
    mci_send(&open_command)?;
    let result = mci_send(&format!("play {alias} wait"));
    let _ = mci_send(&format!("close {alias}"));
    result
}

#[cfg(target_os = "windows")]
fn mci_send(command: &str) -> Result<(), String> {
    use std::os::windows::ffi::OsStrExt as _;

    #[link(name = "winmm")]
    unsafe extern "system" {
        fn mciSendStringW(
            command: *const u16,
            return_value: *mut u16,
            return_length: u32,
            callback: isize,
        ) -> u32;
        fn mciGetErrorStringW(error: u32, text: *mut u16, length: u32) -> i32;
    }

    let wide = std::ffi::OsStr::new(command)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    // SAFETY: `wide` is NUL-terminated and remains alive for the duration of the call.
    let error = unsafe { mciSendStringW(wide.as_ptr(), std::ptr::null_mut(), 0, 0) };
    if error == 0 {
        return Ok(());
    }
    let mut buffer = [0_u16; 256];
    // SAFETY: `buffer` is valid for 256 UTF-16 code units.
    let has_message =
        unsafe { mciGetErrorStringW(error, buffer.as_mut_ptr(), buffer.len() as u32) };
    let detail = if has_message != 0 {
        let length = buffer
            .iter()
            .position(|value| *value == 0)
            .unwrap_or(buffer.len());
        String::from_utf16_lossy(&buffer[..length])
    } else {
        format!("errore MCI {error}")
    };
    Err(format!("Windows non riesce a riprodurre la voce: {detail}"))
}

#[cfg(target_os = "macos")]
fn list_system_voices_blocking() -> Result<Vec<TtsVoiceOption>, String> {
    let output = Command::new("/usr/bin/say")
        .args(["-v", "?"])
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .map_err(|_| "La sintesi vocale macOS non è disponibile.".to_string())?;
    if !output.status.success() {
        return Err("La sintesi vocale macOS non è disponibile.".into());
    }
    let voices = parse_macos_voice_list(&String::from_utf8_lossy(&output.stdout));
    if voices.is_empty() {
        Err("macOS non ha restituito voci installate.".into())
    } else {
        Ok(voices)
    }
}

#[cfg(target_os = "macos")]
fn parse_macos_voice_list(output: &str) -> Vec<TtsVoiceOption> {
    output
        .lines()
        .filter_map(|line| {
            let fields = line.split_whitespace().collect::<Vec<_>>();
            let locale_index = fields
                .iter()
                .position(|field| field.len() >= 2 && (field.contains('_') || *field == "en"))?;
            if locale_index == 0 {
                return None;
            }
            let name = fields[..locale_index].join(" ");
            Some(TtsVoiceOption {
                id: name.clone(),
                name,
                provider: "system".into(),
                language: Some(fields[locale_index].into()),
                local: true,
            })
        })
        .collect()
}

#[cfg(target_os = "macos")]
fn speak_system_blocking(text: &str, voice: Option<&str>, speed: f32) -> Result<(), String> {
    let mut command = Command::new("/usr/bin/say");
    if let Some(voice) = voice {
        command.args(["-v", voice]);
    }
    command
        .args([
            "-r",
            &((200.0 * speed).round() as u32).clamp(80, 500).to_string(),
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    let mut child = command
        .spawn()
        .map_err(|_| "La sintesi vocale macOS non è disponibile.".to_string())?;
    let Some(mut stdin) = child.stdin.take() else {
        let _ = child.kill();
        return Err("Non riesco a inviare il testo alla voce macOS.".into());
    };
    if stdin.write_all(text.as_bytes()).is_err() {
        let _ = child.kill();
        return Err("Non riesco a inviare il testo alla voce macOS.".into());
    }
    drop(stdin);
    let status = child
        .wait()
        .map_err(|_| "La sintesi vocale macOS si è interrotta.".to_string())?;
    if status.success() {
        Ok(())
    } else {
        Err("La voce macOS selezionata non è disponibile.".into())
    }
}

#[cfg(target_os = "macos")]
fn play_audio_file(path: &Path) -> Result<(), String> {
    let status = Command::new("/usr/bin/afplay")
        .arg(path)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map_err(|_| "Il player audio macOS non è disponibile.".to_string())?;
    if status.success() {
        Ok(())
    } else {
        Err("macOS non riesce a riprodurre la voce.".into())
    }
}

#[cfg(not(any(target_os = "windows", target_os = "macos")))]
fn list_system_voices_blocking() -> Result<Vec<TtsVoiceOption>, String> {
    Err("Le voci di sistema Linux non sono ancora supportate.".into())
}

#[cfg(not(any(target_os = "windows", target_os = "macos")))]
fn speak_system_blocking(_text: &str, _voice: Option<&str>, _speed: f32) -> Result<(), String> {
    Err("La sintesi vocale Linux non è ancora supportata.".into())
}

#[cfg(not(any(target_os = "windows", target_os = "macos")))]
fn play_audio_file(_path: &Path) -> Result<(), String> {
    Err("La riproduzione vocale Linux non è ancora supportata.".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_is_valid() {
        assert!(normalize_config(TtsConfig::default()).is_ok());
    }

    #[test]
    fn invalid_config_is_rejected() {
        let mut config = TtsConfig::default();
        config.provider = "browser".into();
        assert!(normalize_config(config).is_err());

        let mut config = TtsConfig::default();
        config.speed = f32::NAN;
        assert!(normalize_config(config).is_err());

        let mut config = TtsConfig::default();
        config.voice = "unknown-openai-voice".into();
        assert!(normalize_config(config).is_err());
    }

    #[test]
    fn selected_model_is_sent_to_each_provider() {
        let mut openai = TtsConfig::default();
        openai.model = "tts-1-hd".into();
        let body = openai_request_body(&openai, "Ciao");
        assert_eq!(body["model"], "tts-1-hd");
        assert!(body.get("instructions").is_none());

        let mut openrouter = TtsConfig::default();
        openrouter.provider = "openrouter".into();
        openrouter.model = "openai/gpt-4o-mini-tts-2025-12-15".into();
        let body = openrouter_request_body(&openrouter, "Ciao");
        assert_eq!(body["model"], "openai/gpt-4o-mini-tts-2025-12-15");
        assert_eq!(
            body["provider"]["options"]["openai"]["instructions"],
            openrouter.instructions.unwrap()
        );
    }

    #[test]
    fn text_limit_counts_unicode_characters() {
        assert!(validate_text(&"è".repeat(MAX_TEXT_CHARACTERS)).is_ok());
        assert!(validate_text(&"è".repeat(MAX_TEXT_CHARACTERS + 1)).is_err());
    }

    #[test]
    fn config_round_trips_without_credentials() {
        let mut random = [0_u8; 8];
        getrandom::fill(&mut random).unwrap();
        let suffix = random
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        let directory = std::env::temp_dir().join(format!("onyx-tts-test-{suffix}"));
        let path = directory.join("tts.json");
        let config = TtsConfig::default();
        save_config(&path, &config).unwrap();
        let loaded = load_config(&path).unwrap();
        assert_eq!(loaded, config);
        let persisted = fs::read_to_string(&path).unwrap();
        assert!(!persisted.contains("apiKey"));
        let _ = fs::remove_dir_all(directory);
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn parses_windows_system_voices() {
        let voices = parse_windows_voice_list("Microsoft Elsa\tit-IT\tFemale\n");
        assert_eq!(voices.len(), 1);
        assert_eq!(voices[0].id, "Microsoft Elsa");
        assert_eq!(voices[0].language.as_deref(), Some("it-IT"));
    }
}
