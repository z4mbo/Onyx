use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tauri::{AppHandle, Emitter};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    time::{Duration, timeout},
};
use url::Url;

use crate::{models::OpenRouterAuthEvent, provider, secrets};

const AUTH_URL: &str = "https://openrouter.ai/auth";
const EXCHANGE_URL: &str = "https://openrouter.ai/api/v1/auth/keys";

#[derive(Serialize)]
struct ExchangeRequest<'a> {
    code: &'a str,
    code_verifier: &'a str,
    code_challenge_method: &'static str,
}

#[derive(Deserialize)]
struct ExchangeResponse {
    key: String,
}

struct BusyReset(Arc<AtomicBool>);

impl Drop for BusyReset {
    fn drop(&mut self) {
        self.0.store(false, Ordering::SeqCst);
    }
}

pub async fn start(app: AppHandle, client: Client, busy: Arc<AtomicBool>) -> Result<(), String> {
    if busy.swap(true, Ordering::SeqCst) {
        return Err("Un accesso OpenRouter è già in corso.".into());
    }

    let listener = match TcpListener::bind("127.0.0.1:0").await {
        Ok(listener) => listener,
        Err(error) => {
            busy.store(false, Ordering::SeqCst);
            return Err(format!("Callback OpenRouter non disponibile: {error}"));
        }
    };
    let port = match listener.local_addr() {
        Ok(address) => address.port(),
        Err(error) => {
            busy.store(false, Ordering::SeqCst);
            return Err(format!("Callback OpenRouter non disponibile: {error}"));
        }
    };
    let verifier = match create_verifier() {
        Ok(verifier) => verifier,
        Err(error) => {
            busy.store(false, Ordering::SeqCst);
            return Err(error);
        }
    };
    let callback = format!("http://127.0.0.1:{port}/callback");
    let authorization_url = match build_authorization_url(&callback, &pkce_challenge(&verifier)) {
        Ok(url) => url,
        Err(error) => {
            busy.store(false, Ordering::SeqCst);
            return Err(error);
        }
    };

    // Il listener è già attivo: il browser non può arrivare prima del bind.
    if let Err(error) = tauri_plugin_opener::open_url(&authorization_url, None::<&str>) {
        busy.store(false, Ordering::SeqCst);
        return Err(format!("Non riesco ad aprire il browser: {error}"));
    }
    emit(&app, "waiting", Some("Completa l'accesso nel browser."));

    tauri::async_runtime::spawn(async move {
        let _reset = BusyReset(busy);
        let outcome = complete(listener, &client, &verifier).await;
        match outcome {
            Ok(()) => emit(
                &app,
                "connected",
                Some("OpenRouter è collegato e pronto per la dettatura."),
            ),
            Err(error) => emit(&app, "error", Some(&error)),
        }
    });
    Ok(())
}

async fn complete(listener: TcpListener, client: &Client, verifier: &str) -> Result<(), String> {
    let (mut stream, _) = timeout(Duration::from_secs(180), listener.accept())
        .await
        .map_err(|_| "Accesso OpenRouter scaduto. Riprova dalle impostazioni.".to_string())?
        .map_err(|error| format!("Callback OpenRouter non riuscito: {error}"))?;

    let code = match read_callback_code(&mut stream).await {
        Ok(code) => {
            let _ = write_browser_page(
                &mut stream,
                "Accesso completato",
                "Puoi chiudere questa pagina e tornare a Onyx.",
            )
            .await;
            code
        }
        Err(error) => {
            let _ = write_browser_page(
                &mut stream,
                "Accesso non completato",
                "Torna a Onyx e riprova.",
            )
            .await;
            return Err(error);
        }
    };

    let response = client
        .post(EXCHANGE_URL)
        .json(&ExchangeRequest {
            code: &code,
            code_verifier: verifier,
            code_challenge_method: "S256",
        })
        .send()
        .await
        .map_err(|error| format!("Scambio OAuth OpenRouter non riuscito: {error}"))?;
    let status = response.status();
    let body = response
        .text()
        .await
        .map_err(|error| format!("Risposta OAuth OpenRouter non leggibile: {error}"))?;
    if !status.is_success() {
        return Err(format!(
            "OpenRouter non ha completato l'accesso (HTTP {}).",
            status.as_u16()
        ));
    }
    let exchange: ExchangeResponse = serde_json::from_str(&body)
        .map_err(|_| "OpenRouter ha restituito una credenziale non valida.".to_string())?;
    provider::validate_key(client, &exchange.key).await?;
    secrets::set_openrouter_key(&exchange.key)
}

fn create_verifier() -> Result<String, String> {
    let mut random = [0_u8; 32];
    getrandom::fill(&mut random)
        .map_err(|error| format!("Generatore OAuth non disponibile: {error}"))?;
    Ok(URL_SAFE_NO_PAD.encode(random))
}

fn pkce_challenge(verifier: &str) -> String {
    URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()))
}

fn build_authorization_url(callback: &str, challenge: &str) -> Result<String, String> {
    let mut url = Url::parse(AUTH_URL).map_err(|error| error.to_string())?;
    url.query_pairs_mut()
        .append_pair("callback_url", callback)
        .append_pair("code_challenge", challenge)
        .append_pair("code_challenge_method", "S256");
    Ok(url.to_string())
}

async fn read_callback_code(stream: &mut TcpStream) -> Result<String, String> {
    let mut buffer = [0_u8; 16 * 1024];
    let bytes = timeout(Duration::from_secs(15), stream.read(&mut buffer))
        .await
        .map_err(|_| "Il browser non ha completato il callback OpenRouter.".to_string())?
        .map_err(|error| format!("Callback OpenRouter non leggibile: {error}"))?;
    if bytes == 0 {
        return Err("Callback OpenRouter vuoto.".into());
    }
    let request = String::from_utf8_lossy(&buffer[..bytes]);
    let target = request
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .ok_or_else(|| "Callback OpenRouter non valido.".to_string())?;
    parse_callback_target(target)
}

fn parse_callback_target(target: &str) -> Result<String, String> {
    let url = Url::parse(&format!("http://127.0.0.1{target}"))
        .map_err(|_| "Callback OpenRouter non valido.".to_string())?;
    if url.path() != "/callback" {
        return Err("Percorso callback OpenRouter non valido.".into());
    }
    let mut code = None;
    let mut oauth_error = None;
    for (key, value) in url.query_pairs() {
        match key.as_ref() {
            "code" => code = Some(value.into_owned()),
            "error" | "error_description" => oauth_error = Some(value.into_owned()),
            _ => {}
        }
    }
    if let Some(error) = oauth_error {
        return Err(format!(
            "Accesso OpenRouter annullato: {}",
            truncate(&error, 160)
        ));
    }
    code.filter(|value| !value.is_empty() && value.len() <= 2_048)
        .ok_or_else(|| "OpenRouter non ha restituito il codice di accesso.".to_string())
}

async fn write_browser_page(
    stream: &mut TcpStream,
    title: &str,
    message: &str,
) -> Result<(), std::io::Error> {
    let body = format!(
        "<!doctype html><html lang=\"it\"><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width\"><title>{title}</title><style>body{{margin:0;background:#08090b;color:#f6f7f8;font:16px system-ui;display:grid;place-items:center;min-height:100vh}}main{{max-width:420px;padding:36px;text-align:center}}h1{{font-size:27px}}p{{color:#9aa0aa;line-height:1.6}}</style><main><h1>{title}</h1><p>{message}</p></main></html>"
    );
    let response = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\nCache-Control: no-store\r\n\r\n{}",
        body.len(),
        body
    );
    stream.write_all(response.as_bytes()).await
}

fn emit(app: &AppHandle, status: &str, message: Option<&str>) {
    let _ = app.emit(
        "onyx://openrouter-auth",
        OpenRouterAuthEvent {
            status: status.into(),
            message: message.map(str::to_owned),
        },
    );
}

fn truncate(value: &str, limit: usize) -> String {
    value.chars().take(limit).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pkce_matches_rfc_7636_vector() {
        let verifier = "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk";
        assert_eq!(
            pkce_challenge(verifier),
            "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"
        );
    }

    #[test]
    fn generated_verifier_has_pkce_length() {
        let verifier = create_verifier().unwrap();
        assert_eq!(verifier.len(), 43);
        assert!(
            verifier
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
        );
    }

    #[test]
    fn callback_parser_accepts_only_expected_path_and_code() {
        assert_eq!(
            parse_callback_target("/callback?code=abc%2D123").unwrap(),
            "abc-123"
        );
        assert!(parse_callback_target("/favicon.ico").is_err());
        assert!(parse_callback_target("/callback").is_err());
        assert!(parse_callback_target("/callback?error=access_denied").is_err());
    }
}
