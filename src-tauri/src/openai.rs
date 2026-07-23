use crate::model::{ChatMedia, ChatReply, OpenAiStatus, TranscriptionReply, TranscriptionRequest};
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use reqwest::{
    Client, Response, StatusCode,
    multipart::{Form, Part},
};
use serde_json::{Value, json};
use std::time::Duration;

const SERVICE: &str = "com.z4mbo.onyx";
const ACCOUNT: &str = "openai-api";
const API_BASE: &str = "https://api.openai.com/v1";
const MAX_KEY_BYTES: usize = 8192;
const MAX_AUDIO_BYTES: usize = 24 * 1024 * 1024;
const MAX_IMAGE_RESPONSE_BYTES: usize = 32 * 1024 * 1024;
const MAX_SPEECH_BYTES: usize = 24 * 1024 * 1024;
const MAX_ERROR_BYTES: usize = 16 * 1024;

pub async fn status() -> OpenAiStatus {
    OpenAiStatus {
        connected: read_key().await.is_ok_and(|value| !value.trim().is_empty()),
    }
}

pub async fn save_key(value: String) -> Result<OpenAiStatus, String> {
    let key = value.trim().to_string();
    if key.is_empty() || key.len() > MAX_KEY_BYTES {
        return Err("Enter a valid OpenAI API key".to_string());
    }
    validate_key(&key).await?;
    tokio::task::spawn_blocking(move || {
        keyring::Entry::new(SERVICE, ACCOUNT)
            .map_err(|error| error.to_string())?
            .set_password(&key)
            .map_err(|error| error.to_string())
    })
    .await
    .map_err(|error| error.to_string())??;
    Ok(OpenAiStatus { connected: true })
}

pub async fn clear_key() -> Result<OpenAiStatus, String> {
    tokio::task::spawn_blocking(|| {
        let entry = keyring::Entry::new(SERVICE, ACCOUNT).map_err(|error| error.to_string())?;
        match entry.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(error) => Err(error.to_string()),
        }
    })
    .await
    .map_err(|error| error.to_string())??;
    Ok(OpenAiStatus { connected: false })
}

pub async fn generate_image(
    model: &str,
    prompt: &str,
    aspect_ratio: Option<&str>,
) -> Result<ChatReply, String> {
    if prompt.trim().is_empty() || prompt.len() > 256 * 1024 {
        return Err("The image prompt is empty or too long".to_string());
    }
    if model.is_empty() || model.len() > 256 {
        return Err("The OpenAI image model is invalid".to_string());
    }
    let size = image_size(aspect_ratio);
    let key = read_key().await?;
    let response = client(Duration::from_secs(300))?
        .post(format!("{API_BASE}/images/generations"))
        .bearer_auth(key)
        .json(&json!({
            "model": model,
            "prompt": prompt,
            "size": size,
            "quality": "auto",
            "output_format": "png",
            "n": 1
        }))
        .send()
        .await
        .map_err(|error| format!("OpenAI image request failed: {error}"))?;
    let body = success_body(response, MAX_IMAGE_RESPONSE_BYTES).await?;
    let value: Value = serde_json::from_slice(&body)
        .map_err(|_| "OpenAI returned invalid image JSON".to_string())?;
    let data = value
        .pointer("/data/0")
        .ok_or_else(|| "OpenAI returned no generated image".to_string())?;
    let url = if let Some(encoded) = data.get("b64_json").and_then(Value::as_str) {
        format!("data:image/png;base64,{encoded}")
    } else {
        data.get("url")
            .and_then(Value::as_str)
            .map(str::to_string)
            .ok_or_else(|| "OpenAI returned an unsupported image payload".to_string())?
    };
    Ok(ChatReply {
        content: "Image generated with OpenAI".to_string(),
        model: model.to_string(),
        media: vec![ChatMedia {
            kind: "image".to_string(),
            url,
            mime_type: Some("image/png".to_string()),
        }],
    })
}

fn image_size(aspect_ratio: Option<&str>) -> &'static str {
    match aspect_ratio.unwrap_or("1:1") {
        "16:9" | "4:3" | "3:2" => "1536x1024",
        "9:16" | "3:4" | "2:3" => "1024x1536",
        _ => "1024x1024",
    }
}

pub async fn transcribe(
    model: &str,
    language: Option<&str>,
    request: &TranscriptionRequest,
) -> Result<TranscriptionReply, String> {
    let audio = BASE64
        .decode(&request.audio_base64)
        .map_err(|_| "The recording is not valid base64 audio".to_string())?;
    if audio.is_empty() || audio.len() > MAX_AUDIO_BYTES {
        return Err("The recording is empty or exceeds the 24 MiB limit".to_string());
    }
    let extension = match request.format.as_str() {
        "mp4" => "mp4",
        "ogg" => "ogg",
        _ => "webm",
    };
    let mime = match extension {
        "mp4" => "audio/mp4",
        "ogg" => "audio/ogg",
        _ => "audio/webm",
    };
    let file = Part::bytes(audio)
        .file_name(format!("recording.{extension}"))
        .mime_str(mime)
        .map_err(|error| error.to_string())?;
    let mut form = Form::new()
        .text("model", model.to_string())
        .part("file", file);
    if let Some(language) = language.filter(|value| !value.trim().is_empty()) {
        form = form.text("language", language.to_string());
    }
    let key = read_key().await?;
    let response = client(Duration::from_secs(180))?
        .post(format!("{API_BASE}/audio/transcriptions"))
        .bearer_auth(key)
        .multipart(form)
        .send()
        .await
        .map_err(|error| format!("OpenAI transcription failed: {error}"))?;
    let body = success_body(response, 2 * 1024 * 1024).await?;
    let value: Value = serde_json::from_slice(&body)
        .map_err(|_| "OpenAI returned invalid transcription JSON".to_string())?;
    let text = value
        .get("text")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "OpenAI returned an empty transcription".to_string())?
        .to_string();
    Ok(TranscriptionReply {
        text,
        model: model.to_string(),
    })
}

pub async fn speak(model: &str, voice: &str, speed: f32, text: &str) -> Result<String, String> {
    if text.trim().is_empty() || text.len() > 32_000 {
        return Err("Speech text is empty or too long".to_string());
    }
    let key = read_key().await?;
    let response = client(Duration::from_secs(180))?
        .post(format!("{API_BASE}/audio/speech"))
        .bearer_auth(key)
        .json(&json!({
            "model": model,
            "input": text,
            "voice": voice,
            "speed": speed,
            "response_format": "mp3"
        }))
        .send()
        .await
        .map_err(|error| format!("OpenAI speech request failed: {error}"))?;
    let bytes = success_body(response, MAX_SPEECH_BYTES).await?;
    Ok(format!("data:audio/mpeg;base64,{}", BASE64.encode(bytes)))
}

async fn validate_key(key: &str) -> Result<(), String> {
    let response = client(Duration::from_secs(30))?
        .get(format!("{API_BASE}/models"))
        .bearer_auth(key)
        .send()
        .await
        .map_err(|error| format!("Could not validate the OpenAI API key: {error}"))?;
    if response.status().is_success() {
        return Ok(());
    }
    Err(api_error(response).await)
}

async fn read_key() -> Result<String, String> {
    tokio::task::spawn_blocking(|| {
        let entry = keyring::Entry::new(SERVICE, ACCOUNT).map_err(|error| error.to_string())?;
        match entry.get_password() {
            Ok(key) => Ok(key),
            Err(keyring::Error::NoEntry) => {
                Err("Connect an OpenAI API key in Settings → Runtimes".to_string())
            }
            Err(error) => Err(error.to_string()),
        }
    })
    .await
    .map_err(|error| error.to_string())?
}

fn client(timeout: Duration) -> Result<Client, String> {
    Client::builder()
        .timeout(timeout)
        .build()
        .map_err(|error| error.to_string())
}

async fn success_body(response: Response, max_bytes: usize) -> Result<Vec<u8>, String> {
    if !response.status().is_success() {
        return Err(api_error(response).await);
    }
    if response
        .content_length()
        .is_some_and(|length| length > max_bytes as u64)
    {
        return Err("OpenAI response exceeded the Onyx safety limit".to_string());
    }
    let bytes = response
        .bytes()
        .await
        .map_err(|error| format!("Could not read the OpenAI response: {error}"))?;
    if bytes.len() > max_bytes {
        return Err("OpenAI response exceeded the Onyx safety limit".to_string());
    }
    Ok(bytes.to_vec())
}

async fn api_error(response: Response) -> String {
    let status = response.status();
    let body = response.bytes().await.unwrap_or_default();
    let bounded = &body[..body.len().min(MAX_ERROR_BYTES)];
    let value = serde_json::from_slice::<Value>(bounded).ok();
    let detail = value
        .as_ref()
        .and_then(|value| value.pointer("/error/message"))
        .and_then(Value::as_str)
        .unwrap_or_else(|| std::str::from_utf8(bounded).unwrap_or("Unknown OpenAI error"));
    match status {
        StatusCode::UNAUTHORIZED => "OpenAI rejected the API key".to_string(),
        _ => format!("OpenAI request failed ({status}): {detail}"),
    }
}

#[cfg(test)]
mod tests {
    use super::image_size;

    #[test]
    fn image_ratios_map_to_supported_gpt_image_sizes() {
        assert_eq!(image_size(Some("1:1")), "1024x1024");
        assert_eq!(image_size(Some("16:9")), "1536x1024");
        assert_eq!(image_size(Some("9:16")), "1024x1536");
        assert_eq!(image_size(Some("unexpected")), "1024x1024");
    }
}
