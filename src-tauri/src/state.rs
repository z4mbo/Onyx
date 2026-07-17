use std::{
    path::PathBuf,
    sync::{Arc, RwLock, atomic::AtomicBool},
};

use crate::{
    codex::CodexState,
    models::{AppSettings, TtsConfig},
    tts,
};

pub struct AppState {
    pub settings: RwLock<AppSettings>,
    pub client: reqwest::Client,
    pub oauth_in_progress: Arc<AtomicBool>,
    pub tts_config: RwLock<TtsConfig>,
    pub tts_config_path: PathBuf,
    pub tts_in_progress: AtomicBool,
    pub codex: CodexState,
}

impl AppState {
    pub fn new(config_dir: PathBuf) -> Result<Self, String> {
        let client = reqwest::Client::builder()
            .connect_timeout(std::time::Duration::from_secs(15))
            .timeout(std::time::Duration::from_secs(120))
            .user_agent(format!("Onyx/{}", env!("CARGO_PKG_VERSION")))
            .build()
            .map_err(|error| format!("Client OpenRouter non disponibile: {error}"))?;
        let tts_config_path = config_dir.join("tts.json");
        let tts_config = tts::load_config(&tts_config_path).unwrap_or_else(|error| {
            eprintln!("Onyx TTS: {error} Uso la configurazione predefinita.");
            TtsConfig::default()
        });
        Ok(Self {
            settings: RwLock::new(AppSettings::default()),
            client,
            oauth_in_progress: Arc::new(AtomicBool::new(false)),
            tts_config: RwLock::new(tts_config),
            tts_config_path,
            tts_in_progress: AtomicBool::new(false),
            codex: CodexState::default(),
        })
    }
}

pub fn platform_config_dir() -> PathBuf {
    #[cfg(target_os = "windows")]
    {
        return std::env::var_os("APPDATA")
            .map(PathBuf::from)
            .unwrap_or_else(std::env::temp_dir)
            .join("com.onyx.assistant");
    }
    #[cfg(target_os = "macos")]
    {
        return std::env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_else(std::env::temp_dir)
            .join("Library")
            .join("Application Support")
            .join("com.onyx.assistant");
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))
            .unwrap_or_else(std::env::temp_dir)
            .join("com.onyx.assistant")
    }
}
