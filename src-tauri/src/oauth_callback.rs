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
use uuid::Uuid;

const CALLBACK_TIMEOUT: Duration = Duration::from_secs(5 * 60);
const POLL_INTERVAL: Duration = Duration::from_millis(25);
const MAX_REQUEST_BYTES: usize = 16 * 1024;

pub fn start(app: AppHandle, cancelled: Arc<AtomicBool>) -> Result<String, String> {
    let listener = TcpListener::bind(("127.0.0.1", 0))
        .map_err(|error| format!("Unable to start the sign-in callback: {error}"))?;
    listener
        .set_nonblocking(true)
        .map_err(|error| format!("Unable to configure the sign-in callback: {error}"))?;
    let address = listener
        .local_addr()
        .map_err(|error| format!("Unable to read the sign-in callback address: {error}"))?;
    let path = format!("/callback/{}", Uuid::new_v4());
    let callback_url = format!("http://127.0.0.1:{}{path}", address.port());
    let callback_url_for_thread = callback_url.clone();

    std::thread::spawn(move || {
        let deadline = Instant::now() + CALLBACK_TIMEOUT;
        while !cancelled.load(Ordering::SeqCst) && Instant::now() < deadline {
            match listener.accept() {
                Ok((mut stream, _)) => {
                    if let Some(target) = read_request_target(&mut stream)
                        && target_matches(&target, &path)
                    {
                        let _ = write_response(&mut stream, 200, success_page());
                        let completed_url = format!(
                            "{}{}",
                            callback_url_for_thread,
                            target.strip_prefix(&path).unwrap_or_default()
                        );
                        let _ = app.emit("onyx://oauth-callback", completed_url);
                        let _ = crate::windowing::show_main(&app);
                        break;
                    }
                    let _ = write_response(&mut stream, 404, not_found_page());
                }
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    std::thread::sleep(POLL_INTERVAL);
                }
                Err(_) => break,
            }
        }
    });

    Ok(callback_url)
}

fn read_request_target(stream: &mut TcpStream) -> Option<String> {
    let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
    let mut buffer = [0_u8; MAX_REQUEST_BYTES];
    let bytes_read = stream.read(&mut buffer).ok()?;
    parse_request_target(&buffer[..bytes_read]).map(str::to_owned)
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

fn target_matches(target: &str, expected_path: &str) -> bool {
    target == expected_path
        || target
            .strip_prefix(expected_path)
            .is_some_and(|suffix| suffix.starts_with('?') || suffix.starts_with('#'))
}

fn write_response(stream: &mut TcpStream, status: u16, body: &str) -> std::io::Result<()> {
    let reason = if status == 200 { "OK" } else { "Not Found" };
    let response = format!(
        "HTTP/1.1 {status} {reason}\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nCache-Control: no-store\r\nConnection: close\r\nX-Content-Type-Options: nosniff\r\n\r\n{body}",
        body.len()
    );
    stream.write_all(response.as_bytes())?;
    stream.flush()
}

fn success_page() -> &'static str {
    r#"<!doctype html><html lang="en"><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><title>Signed in to Onyx</title><style>html{color-scheme:light dark}body{margin:0;min-height:100vh;display:grid;place-items:center;background:#f7f7f6;color:#20201f;font:14px/1.5 system-ui,sans-serif}.card{text-align:center;padding:40px}.orb{width:54px;height:54px;margin:0 auto 18px;border-radius:50%;background:radial-gradient(circle at 30% 24%,#d9ffff 0,#64dcef 22%,#6f72ef 55%,#27194e 100%);box-shadow:0 16px 42px #5146bf38}h1{font-size:22px;letter-spacing:-.03em;margin:0 0 7px}p{margin:0;color:#6f6f6c}@media(prefers-color-scheme:dark){body{background:#161615;color:#f1f1ef}p{color:#aaa9a5}}</style><main class="card"><div class="orb"></div><h1>Authentication successful</h1><p>You can close this window and return to Onyx.</p></main></html>"#
}

fn not_found_page() -> &'static str {
    "<!doctype html><html lang=\"en\"><meta charset=\"utf-8\"><title>Not found</title><p>Not found.</p></html>"
}

#[cfg(test)]
mod tests {
    use super::{parse_request_target, target_matches};

    #[test]
    fn parses_a_get_request_target() {
        assert_eq!(
            parse_request_target(
                b"GET /callback/abc?code=123&state=xyz HTTP/1.1\r\nHost: 127.0.0.1\r\n"
            ),
            Some("/callback/abc?code=123&state=xyz")
        );
        assert_eq!(
            parse_request_target(b"POST /callback/abc HTTP/1.1\r\n"),
            None
        );
    }

    #[test]
    fn only_accepts_the_random_callback_path() {
        assert!(target_matches("/callback/abc?code=123", "/callback/abc"));
        assert!(target_matches("/callback/abc", "/callback/abc"));
        assert!(!target_matches("/callback/abc-spoof", "/callback/abc"));
        assert!(!target_matches("/callback/other?code=123", "/callback/abc"));
    }
}
