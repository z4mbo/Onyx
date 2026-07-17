use keyring::{Entry, Error as KeyringError};

const SERVICE: &str = "com.onyx.assistant";

fn account(provider: &str) -> Result<&'static str, String> {
    match provider {
        "openrouter" => Ok("openrouter-api-key"),
        "openai" => Ok("openai-api-key"),
        "anthropic_api" => Ok("anthropic-api-key"),
        _ => Err("Provider credenziali non supportato.".into()),
    }
}

fn entry(provider: &str) -> Result<Entry, String> {
    Entry::new(SERVICE, account(provider)?)
        .map_err(|error| format!("Archivio credenziali non disponibile: {error}"))
}

pub fn get_provider_key(provider: &str) -> Result<Option<String>, String> {
    match entry(provider)?.get_password() {
        Ok(value) if !value.trim().is_empty() => Ok(Some(value)),
        Ok(_) | Err(KeyringError::NoEntry) => Ok(None),
        Err(error) => Err(format!(
            "Non riesco a leggere la credenziale {provider}: {error}"
        )),
    }
}

pub fn set_provider_key(provider: &str, value: &str) -> Result<(), String> {
    entry(provider)?
        .set_password(value)
        .map_err(|error| format!("Non riesco a salvare la credenziale {provider}: {error}"))
}

pub fn delete_provider_key(provider: &str) -> Result<(), String> {
    match entry(provider)?.delete_credential() {
        Ok(()) | Err(KeyringError::NoEntry) => Ok(()),
        Err(error) => Err(format!(
            "Non riesco a rimuovere la credenziale {provider}: {error}"
        )),
    }
}

pub fn get_openrouter_key() -> Result<Option<String>, String> {
    get_provider_key("openrouter")
}

pub fn set_openrouter_key(value: &str) -> Result<(), String> {
    set_provider_key("openrouter", value)
}

pub fn delete_openrouter_key() -> Result<(), String> {
    delete_provider_key("openrouter")
}
