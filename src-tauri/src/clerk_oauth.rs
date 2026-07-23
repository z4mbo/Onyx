use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use chrono::Utc;
use reqwest::{Client, StatusCode};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};
use tauri::{AppHandle, Emitter};
use url::Url;
use uuid::Uuid;

const SERVICE: &str = "com.z4mbo.onyx";
const ACCOUNT: &str = "clerk-oauth";
const CLIENT_ID: &str = "5d25P5uFMQsN6W2D";
const AUTHORIZE_URL: &str = "https://first-gelding-68.clerk.accounts.dev/oauth/authorize";
const TOKEN_URL: &str = "https://first-gelding-68.clerk.accounts.dev/oauth/token";
const REVOKE_URL: &str = "https://first-gelding-68.clerk.accounts.dev/oauth/token/revoke";
const USERINFO_URL: &str = "https://first-gelding-68.clerk.accounts.dev/oauth/userinfo";
const SCOPES: &str = "openid profile email offline_access";
const CALLBACK_PATH: &str = "/callback";
const CALLBACK_TIMEOUT: Duration = Duration::from_secs(10 * 60);
const POLL_INTERVAL: Duration = Duration::from_millis(25);
const HTTP_TIMEOUT: Duration = Duration::from_secs(30);
const MAX_REQUEST_BYTES: usize = 16 * 1024;
const MAX_RESPONSE_BYTES: usize = 512 * 1024;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountProfile {
    pub id: String,
    pub name: String,
    pub email: String,
    pub image_url: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OAuthStart {
    pub authorize_url: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct AccountEvent {
    profile: Option<AccountProfile>,
    error: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct StoredCredentials {
    access_token: String,
    refresh_token: Option<String>,
    id_token: String,
    access_expires_at: i64,
    profile: AccountProfile,
}

#[derive(Debug, Deserialize)]
struct TokenResponse {
    access_token: String,
    refresh_token: Option<String>,
    id_token: Option<String>,
    expires_in: Option<i64>,
}

#[derive(Debug, Deserialize)]
struct OAuthErrorResponse {
    error: Option<String>,
    error_description: Option<String>,
}

#[derive(Debug, Deserialize)]
struct UserInfo {
    sub: String,
    name: Option<String>,
    preferred_username: Option<String>,
    email: Option<String>,
    picture: Option<String>,
}

struct CallbackRequest {
    stream: TcpStream,
    code: String,
}

pub fn start(
    app: AppHandle,
    cancelled: Arc<AtomicBool>,
    login_hint: Option<String>,
) -> Result<OAuthStart, String> {
    let listener = TcpListener::bind(("127.0.0.1", 0))
        .map_err(|error| format!("Unable to start browser sign-in: {error}"))?;
    listener
        .set_nonblocking(true)
        .map_err(|error| format!("Unable to configure browser sign-in: {error}"))?;
    let port = listener
        .local_addr()
        .map_err(|error| format!("Unable to read the browser sign-in address: {error}"))?
        .port();
    let redirect_uri = format!("http://127.0.0.1:{port}{CALLBACK_PATH}");
    let state = random_urlsafe();
    let verifier = random_urlsafe();
    let challenge = pkce_challenge(&verifier);
    let authorize_url =
        build_authorize_url(&redirect_uri, &state, &challenge, login_hint.as_deref())?;

    tauri::async_runtime::spawn(async move {
        let callback_state = state.clone();
        let callback_cancelled = cancelled.clone();
        let callback = tokio::task::spawn_blocking(move || {
            wait_for_callback(listener, &callback_state, callback_cancelled)
        })
        .await
        .map_err(|error| format!("Browser sign-in task failed: {error}"))
        .and_then(|value| value);

        if cancelled.load(Ordering::SeqCst) {
            return;
        }

        let event = match callback {
            Ok(mut request) => {
                let result = exchange_code(&request.code, &redirect_uri, &verifier).await;
                match result {
                    Ok(profile) => {
                        let _ = write_response(&mut request.stream, 200, success_page());
                        AccountEvent {
                            profile: Some(profile),
                            error: None,
                        }
                    }
                    Err(error) => {
                        let _ = write_response(&mut request.stream, 400, failure_page());
                        AccountEvent {
                            profile: None,
                            error: Some(error),
                        }
                    }
                }
            }
            Err(error) => AccountEvent {
                profile: None,
                error: Some(error),
            },
        };
        let _ = app.emit("onyx://account-changed", event);
        let _ = crate::windowing::show_main(&app);
    });

    Ok(OAuthStart { authorize_url })
}

pub async fn profile() -> Result<Option<AccountProfile>, String> {
    Ok(read_credentials()
        .await?
        .map(|credentials| credentials.profile))
}

pub async fn id_token(force_refresh: bool) -> Result<Option<String>, String> {
    let Some(credentials) = read_credentials().await? else {
        return Ok(None);
    };
    if !force_refresh && jwt_expires_after(&credentials.id_token, 60) {
        return Ok(Some(credentials.id_token));
    }
    let refreshed = refresh_credentials(credentials).await?;
    Ok(Some(refreshed.id_token))
}

pub async fn sign_out() -> Result<(), String> {
    let stored = read_credentials().await?;
    if let Some(credentials) = stored {
        let token = credentials
            .refresh_token
            .as_deref()
            .unwrap_or(&credentials.access_token);
        if let Ok(client) = http_client() {
            let _ = client
                .post(REVOKE_URL)
                .form(&[("client_id", CLIENT_ID), ("token", token)])
                .send()
                .await;
        }
    }
    delete_credentials().await
}

fn build_authorize_url(
    redirect_uri: &str,
    state: &str,
    challenge: &str,
    login_hint: Option<&str>,
) -> Result<String, String> {
    let mut url = Url::parse(AUTHORIZE_URL).map_err(|error| error.to_string())?;
    {
        let mut query = url.query_pairs_mut();
        query
            .append_pair("client_id", CLIENT_ID)
            .append_pair("response_type", "code")
            .append_pair("response_mode", "query")
            .append_pair("redirect_uri", redirect_uri)
            .append_pair("scope", SCOPES)
            .append_pair("state", state)
            .append_pair("code_challenge", challenge)
            .append_pair("code_challenge_method", "S256");
        if let Some(hint) = login_hint
            .map(str::trim)
            .filter(|value| !value.is_empty() && value.len() <= 320)
        {
            query.append_pair("login_hint", hint);
        }
    }
    Ok(url.to_string())
}

fn random_urlsafe() -> String {
    let mut bytes = Vec::with_capacity(48);
    for _ in 0..3 {
        bytes.extend_from_slice(Uuid::new_v4().as_bytes());
    }
    URL_SAFE_NO_PAD.encode(bytes)
}

fn pkce_challenge(verifier: &str) -> String {
    URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()))
}

fn wait_for_callback(
    listener: TcpListener,
    expected_state: &str,
    cancelled: Arc<AtomicBool>,
) -> Result<CallbackRequest, String> {
    let deadline = Instant::now() + CALLBACK_TIMEOUT;
    while !cancelled.load(Ordering::SeqCst) && Instant::now() < deadline {
        match listener.accept() {
            Ok((mut stream, _)) => {
                let Some(target) = read_request_target(&mut stream) else {
                    let _ = write_response(&mut stream, 400, failure_page());
                    continue;
                };
                let parsed = match parse_callback(&target, expected_state) {
                    Ok(Some(code)) => code,
                    Ok(None) => {
                        let _ = write_response(&mut stream, 404, not_found_page());
                        continue;
                    }
                    Err(error) => {
                        let _ = write_response(&mut stream, 400, failure_page());
                        return Err(error);
                    }
                };
                return Ok(CallbackRequest {
                    stream,
                    code: parsed,
                });
            }
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(POLL_INTERVAL);
            }
            Err(error) => return Err(format!("Browser sign-in callback failed: {error}")),
        }
    }
    if cancelled.load(Ordering::SeqCst) {
        Err("Browser sign-in was replaced by a newer attempt".to_string())
    } else {
        Err("Browser sign-in timed out. Please try again.".to_string())
    }
}

fn read_request_target(stream: &mut TcpStream) -> Option<String> {
    let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
    let mut request = Vec::with_capacity(1024);
    let mut buffer = [0_u8; 1024];
    while request.len() < MAX_REQUEST_BYTES {
        let read = stream.read(&mut buffer).ok()?;
        if read == 0 {
            break;
        }
        request.extend_from_slice(&buffer[..read]);
        if request.windows(4).any(|window| window == b"\r\n\r\n") {
            break;
        }
    }
    parse_request_target(&request).map(str::to_owned)
}

fn parse_request_target(request: &[u8]) -> Option<&str> {
    let first_line = std::str::from_utf8(request).ok()?.lines().next()?;
    let mut parts = first_line.split_whitespace();
    if parts.next()? != "GET" {
        return None;
    }
    let target = parts.next()?;
    if !target.starts_with('/') || parts.next()?.split('/').next()? != "HTTP" {
        return None;
    }
    Some(target)
}

fn parse_callback(target: &str, expected_state: &str) -> Result<Option<String>, String> {
    let url = Url::parse(&format!("http://127.0.0.1{target}"))
        .map_err(|_| "The browser returned an invalid sign-in callback".to_string())?;
    if url.path() != CALLBACK_PATH {
        return Ok(None);
    }
    let params = url
        .query_pairs()
        .collect::<std::collections::HashMap<_, _>>();
    if params.get("state").map(|value| value.as_ref()) != Some(expected_state) {
        return Err("The browser sign-in state did not match. Please try again.".to_string());
    }
    if let Some(error) = params.get("error") {
        let detail = params
            .get("error_description")
            .map(|value| value.as_ref())
            .unwrap_or(error.as_ref());
        return Err(format!("Clerk sign-in was not completed: {detail}"));
    }
    let code = params
        .get("code")
        .map(|value| value.as_ref())
        .filter(|value| !value.is_empty() && value.len() <= 4096)
        .ok_or_else(|| "Clerk did not return an authorization code".to_string())?;
    Ok(Some(code.to_string()))
}

async fn exchange_code(
    code: &str,
    redirect_uri: &str,
    verifier: &str,
) -> Result<AccountProfile, String> {
    let client = http_client()?;
    let response = client
        .post(TOKEN_URL)
        .form(&[
            ("grant_type", "authorization_code"),
            ("client_id", CLIENT_ID),
            ("code", code),
            ("redirect_uri", redirect_uri),
            ("code_verifier", verifier),
        ])
        .send()
        .await
        .map_err(|error| format!("Unable to exchange the Clerk authorization code: {error}"))?;
    let token: TokenResponse = decode_response(response, "Clerk token exchange").await?;
    let id_token = token
        .id_token
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "Clerk did not return an OpenID identity token".to_string())?;
    let profile = fetch_profile(&client, &token.access_token).await?;
    let credentials = StoredCredentials {
        access_token: token.access_token,
        refresh_token: token.refresh_token,
        id_token,
        access_expires_at: Utc::now().timestamp() + token.expires_in.unwrap_or(3600).max(60),
        profile: profile.clone(),
    };
    write_credentials(credentials).await?;
    Ok(profile)
}

async fn refresh_credentials(credentials: StoredCredentials) -> Result<StoredCredentials, String> {
    let refresh_token = credentials
        .refresh_token
        .clone()
        .ok_or_else(|| "Your Onyx sign-in expired. Please sign in again.".to_string())?;
    let client = http_client()?;
    let response = client
        .post(TOKEN_URL)
        .form(&[
            ("grant_type", "refresh_token"),
            ("client_id", CLIENT_ID),
            ("refresh_token", refresh_token.as_str()),
        ])
        .send()
        .await
        .map_err(|error| format!("Unable to refresh the Clerk session: {error}"))?;
    let token: TokenResponse = decode_response(response, "Clerk token refresh").await?;
    let refreshed = StoredCredentials {
        access_token: token.access_token,
        refresh_token: token.refresh_token.or(credentials.refresh_token),
        id_token: token.id_token.unwrap_or(credentials.id_token),
        access_expires_at: Utc::now().timestamp() + token.expires_in.unwrap_or(3600).max(60),
        profile: credentials.profile,
    };
    if !jwt_expires_after(&refreshed.id_token, 30) {
        return Err("Clerk refreshed the session without a usable identity token".to_string());
    }
    write_credentials(refreshed.clone()).await?;
    Ok(refreshed)
}

async fn fetch_profile(client: &Client, access_token: &str) -> Result<AccountProfile, String> {
    let response = client
        .get(USERINFO_URL)
        .bearer_auth(access_token)
        .send()
        .await
        .map_err(|error| format!("Unable to read the Clerk account profile: {error}"))?;
    let info: UserInfo = decode_response(response, "Clerk user profile").await?;
    let email = info.email.unwrap_or_default();
    let name = info
        .name
        .or(info.preferred_username)
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| {
            email
                .split('@')
                .next()
                .filter(|value| !value.is_empty())
                .unwrap_or("Onyx user")
                .to_string()
        });
    Ok(AccountProfile {
        id: info.sub,
        name,
        email,
        image_url: info.picture.filter(|value| !value.is_empty()),
    })
}

fn http_client() -> Result<Client, String> {
    Client::builder()
        .timeout(HTTP_TIMEOUT)
        .build()
        .map_err(|error| error.to_string())
}

async fn decode_response<T: for<'de> Deserialize<'de>>(
    response: reqwest::Response,
    context: &str,
) -> Result<T, String> {
    let status = response.status();
    let body = response
        .bytes()
        .await
        .map_err(|error| format!("{context} returned an unreadable response: {error}"))?;
    if body.len() > MAX_RESPONSE_BYTES {
        return Err(format!("{context} returned too much data"));
    }
    if status != StatusCode::OK {
        let error = serde_json::from_slice::<OAuthErrorResponse>(&body).ok();
        let detail = error
            .and_then(|value| value.error_description.or(value.error))
            .unwrap_or_else(|| status.to_string());
        return Err(format!("{context} failed: {detail}"));
    }
    serde_json::from_slice(&body).map_err(|_| format!("{context} returned invalid JSON"))
}

fn jwt_expires_after(token: &str, seconds: i64) -> bool {
    let Some(payload) = token.split('.').nth(1) else {
        return false;
    };
    let Ok(bytes) = URL_SAFE_NO_PAD.decode(payload) else {
        return false;
    };
    let Ok(value) = serde_json::from_slice::<serde_json::Value>(&bytes) else {
        return false;
    };
    value
        .get("exp")
        .and_then(serde_json::Value::as_i64)
        .is_some_and(|expires| expires > Utc::now().timestamp() + seconds)
}

async fn read_credentials() -> Result<Option<StoredCredentials>, String> {
    tokio::task::spawn_blocking(|| {
        let entry = keyring::Entry::new(SERVICE, ACCOUNT).map_err(|error| error.to_string())?;
        match entry.get_password() {
            Ok(value) => serde_json::from_str(&value)
                .map(Some)
                .map_err(|_| "The stored Onyx account session is invalid".to_string()),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(error) => Err(error.to_string()),
        }
    })
    .await
    .map_err(|error| error.to_string())?
}

async fn write_credentials(credentials: StoredCredentials) -> Result<(), String> {
    let value = serde_json::to_string(&credentials).map_err(|error| error.to_string())?;
    tokio::task::spawn_blocking(move || {
        keyring::Entry::new(SERVICE, ACCOUNT)
            .map_err(|error| error.to_string())?
            .set_password(&value)
            .map_err(|error| error.to_string())
    })
    .await
    .map_err(|error| error.to_string())?
}

async fn delete_credentials() -> Result<(), String> {
    tokio::task::spawn_blocking(|| {
        let entry = keyring::Entry::new(SERVICE, ACCOUNT).map_err(|error| error.to_string())?;
        match entry.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(error) => Err(error.to_string()),
        }
    })
    .await
    .map_err(|error| error.to_string())?
}

fn write_response(stream: &mut TcpStream, status: u16, body: &str) -> std::io::Result<()> {
    let reason = match status {
        200 => "OK",
        404 => "Not Found",
        _ => "Bad Request",
    };
    let response = format!(
        "HTTP/1.1 {status} {reason}\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nCache-Control: no-store\r\nConnection: close\r\nX-Content-Type-Options: nosniff\r\nReferrer-Policy: no-referrer\r\n\r\n{body}",
        body.len()
    );
    stream.write_all(response.as_bytes())?;
    stream.flush()
}

fn success_page() -> &'static str {
    r#"<!doctype html><html lang="en"><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><title>Signed in to Onyx</title><style>html{color-scheme:light dark}body{margin:0;min-height:100vh;display:grid;place-items:center;background:#f7f7f6;color:#20201f;font:14px/1.5 system-ui,sans-serif}.card{text-align:center;padding:40px}.orb{width:54px;height:54px;margin:0 auto 18px;border-radius:50%;background:radial-gradient(circle at 30% 24%,#d9ffff 0,#64dcef 22%,#6f72ef 55%,#27194e 100%);box-shadow:0 16px 42px #5146bf38}h1{font-size:22px;letter-spacing:-.03em;margin:0 0 7px}p{margin:0;color:#6f6f6c}@media(prefers-color-scheme:dark){body{background:#161615;color:#f1f1ef}p{color:#aaa9a5}}</style><main class="card"><div class="orb"></div><h1>Authentication successful</h1><p>You can close this window and return to Onyx.</p></main></html>"#
}

fn failure_page() -> &'static str {
    r#"<!doctype html><html lang="en"><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><title>Onyx sign-in failed</title><style>html{color-scheme:light dark}body{margin:0;min-height:100vh;display:grid;place-items:center;background:#f7f7f6;color:#20201f;font:14px/1.5 system-ui,sans-serif}.card{text-align:center;padding:40px}h1{font-size:22px;margin:0 0 7px}p{margin:0;color:#6f6f6c}@media(prefers-color-scheme:dark){body{background:#161615;color:#f1f1ef}p{color:#aaa9a5}}</style><main class="card"><h1>Sign-in could not be completed</h1><p>Return to Onyx for details and try again.</p></main></html>"#
}

fn not_found_page() -> &'static str {
    "<!doctype html><html lang=\"en\"><meta charset=\"utf-8\"><title>Not found</title><p>Not found.</p></html>"
}

#[cfg(test)]
mod tests {
    use super::{CALLBACK_PATH, build_authorize_url, parse_callback, pkce_challenge};

    #[test]
    fn creates_the_rfc_7636_s256_challenge() {
        assert_eq!(
            pkce_challenge("dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk"),
            "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"
        );
    }

    #[test]
    fn authorization_url_uses_pkce_and_loopback_redirect() {
        let value = build_authorize_url(
            "http://127.0.0.1:43210/callback",
            "state-value",
            "challenge-value",
            Some("person@example.com"),
        )
        .unwrap();
        let url = url::Url::parse(&value).unwrap();
        let query = url
            .query_pairs()
            .collect::<std::collections::HashMap<_, _>>();
        assert_eq!(query.get("response_type").unwrap(), "code");
        assert_eq!(query.get("response_mode").unwrap(), "query");
        assert_eq!(query.get("code_challenge_method").unwrap(), "S256");
        assert_eq!(
            query.get("redirect_uri").unwrap(),
            "http://127.0.0.1:43210/callback"
        );
        assert_eq!(query.get("login_hint").unwrap(), "person@example.com");
    }

    #[test]
    fn callback_requires_the_expected_path_and_state() {
        assert_eq!(
            parse_callback(
                &format!("{CALLBACK_PATH}?code=abc&state=expected"),
                "expected"
            )
            .unwrap(),
            Some("abc".to_string())
        );
        assert!(
            parse_callback("/other?code=abc&state=expected", "expected")
                .unwrap()
                .is_none()
        );
        assert!(
            parse_callback(&format!("{CALLBACK_PATH}?code=abc&state=wrong"), "expected").is_err()
        );
    }
}
