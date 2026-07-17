use std::collections::HashSet;

use base64::{Engine as _, engine::general_purpose::STANDARD};
use reqwest::{Client, StatusCode, multipart};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::models::{
    ModelOption, SearchReply, SearchRequest, SearchSource, SearchUsage, TranscriptionReply,
    TranscriptionRequest,
};

const OPENROUTER_BASE: &str = "https://openrouter.ai/api/v1";
const OPENAI_BASE: &str = "https://api.openai.com/v1";
const MAX_AUDIO_BASE64_BYTES: usize = 32 * 1024 * 1024;

#[derive(Serialize)]
struct InputAudio<'a> {
    data: &'a str,
    format: &'a str,
}

#[derive(Serialize)]
struct TranscriptionBody<'a> {
    model: &'a str,
    input_audio: InputAudio<'a>,
    #[serde(skip_serializing_if = "Option::is_none")]
    language: Option<&'a str>,
    temperature: f32,
}

#[derive(Deserialize)]
struct TranscriptionResponse {
    text: String,
    #[serde(default)]
    usage: Option<Value>,
}

#[derive(Deserialize)]
struct ModelsResponse {
    data: Vec<OpenRouterModel>,
}

#[derive(Deserialize)]
struct OpenRouterModel {
    id: String,
    name: String,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    architecture: ModelArchitecture,
    #[serde(default)]
    pricing: ModelPricing,
}

#[derive(Default, Deserialize)]
struct ModelArchitecture {
    #[serde(default)]
    input_modalities: Vec<String>,
    #[serde(default)]
    output_modalities: Vec<String>,
}

#[derive(Default, Deserialize)]
struct ModelPricing {
    #[serde(default)]
    prompt: Option<String>,
    #[serde(default)]
    completion: Option<String>,
}

pub async fn validate_provider_key(
    client: &Client,
    provider: &str,
    api_key: &str,
) -> Result<(), String> {
    validate_api_key_shape(api_key)?;
    let url = match provider {
        "openrouter" => format!("{OPENROUTER_BASE}/key"),
        "openai" => format!("{OPENAI_BASE}/models"),
        "anthropic_api" => "https://api.anthropic.com/v1/models".into(),
        _ => return Err("Questo provider non usa una API key diretta.".into()),
    };
    let mut request = client.get(url).bearer_auth(api_key);
    if provider == "openrouter" {
        request = request.header("X-OpenRouter-Title", "Onyx");
    } else if provider == "anthropic_api" {
        request = client
            .get("https://api.anthropic.com/v1/models")
            .header("x-api-key", api_key)
            .header("anthropic-version", "2023-06-01");
    }
    let response = request
        .send()
        .await
        .map_err(|error| network_error(provider, error))?;
    if response.status().is_success() {
        return Ok(());
    }
    let status = response.status();
    let body = response.text().await.unwrap_or_default();
    Err(api_error(provider, status, &body))
}

pub async fn validate_key(client: &Client, api_key: &str) -> Result<(), String> {
    validate_provider_key(client, "openrouter", api_key).await
}

pub async fn list_models(
    client: &Client,
    provider: &str,
    capability: &str,
    api_key: Option<&str>,
) -> Result<Vec<ModelOption>, String> {
    match provider {
        "openrouter" => list_openrouter_models(client, capability, api_key).await,
        "openai" => list_openai_models(client, capability, api_key).await,
        "anthropic_api" => list_anthropic_models(client, capability, api_key).await,
        "local" => Ok(local_model_placeholders(capability)),
        "managed" => Ok(managed_model_placeholders(capability)),
        "claude_subscription_agent_sdk" => Ok(Vec::new()),
        _ => Err("Provider non supportato.".into()),
    }
}

pub async fn list_transcription_models(
    client: &Client,
    api_key: Option<&str>,
) -> Result<Vec<ModelOption>, String> {
    list_openrouter_models(client, "transcription", api_key).await
}

async fn list_openrouter_models(
    client: &Client,
    capability: &str,
    api_key: Option<&str>,
) -> Result<Vec<ModelOption>, String> {
    let mut request = client
        .get(format!("{OPENROUTER_BASE}/models"))
        .header("X-OpenRouter-Title", "Onyx");
    if capability == "tts" {
        request = request.query(&[("output_modalities", "speech")]);
    }
    if let Some(api_key) = api_key {
        request = request.bearer_auth(api_key);
    }
    let response = request
        .send()
        .await
        .map_err(|error| network_error("OpenRouter", error))?;
    let status = response.status();
    let body = response
        .text()
        .await
        .map_err(|error| network_error("OpenRouter", error))?;
    if !status.is_success() {
        return Err(api_error("OpenRouter", status, &body));
    }
    let payload: ModelsResponse = serde_json::from_str(&body)
        .map_err(|_| "OpenRouter ha restituito un catalogo modelli non valido.".to_string())?;
    let mut models = payload
        .data
        .into_iter()
        .filter(|model| openrouter_supports(model, capability))
        .map(|model| ModelOption {
            id: model.id,
            name: model.name,
            description: model.description.map(|value| truncate(&value, 240)),
            prompt_price: model.pricing.prompt,
            completion_price: model.pricing.completion,
        })
        .collect::<Vec<_>>();
    models.sort_by(|left, right| left.name.cmp(&right.name));
    models.dedup_by(|left, right| left.id == right.id);
    if models.is_empty() {
        return Err("OpenRouter non ha restituito modelli compatibili.".into());
    }
    Ok(models)
}

fn openrouter_supports(model: &OpenRouterModel, capability: &str) -> bool {
    let input = &model.architecture.input_modalities;
    let output = &model.architecture.output_modalities;
    let has_input = |kind: &str| input.is_empty() || input.iter().any(|item| item == kind);
    let has_output = |kind: &str| output.is_empty() || output.iter().any(|item| item == kind);
    match capability {
        "transcription" | "stt" => {
            output.iter().any(|item| item == "transcription")
                || model.id.contains("whisper")
                || model.id.contains("transcribe")
        }
        "web_search" | "computer" | "files" => has_input("text") && has_output("text"),
        "tts" => {
            output
                .iter()
                .any(|item| item == "speech" || item == "audio")
                || model.id.contains("tts")
                || model.id.contains("voice")
        }
        "images" => output.iter().any(|item| item == "image") || model.id.contains("image"),
        "video" => output.iter().any(|item| item == "video") || model.id.contains("video"),
        _ => false,
    }
}

async fn list_openai_models(
    client: &Client,
    capability: &str,
    api_key: Option<&str>,
) -> Result<Vec<ModelOption>, String> {
    let api_key =
        api_key.ok_or_else(|| "Collega una API key OpenAI per caricare i modelli.".to_string())?;
    let response = client
        .get(format!("{OPENAI_BASE}/models"))
        .bearer_auth(api_key)
        .send()
        .await
        .map_err(|error| network_error("OpenAI", error))?;
    let status = response.status();
    let body = response
        .text()
        .await
        .map_err(|error| network_error("OpenAI", error))?;
    if !status.is_success() {
        return Err(api_error("OpenAI", status, &body));
    }
    let payload: Value = serde_json::from_str(&body)
        .map_err(|_| "OpenAI ha restituito un catalogo modelli non valido.".to_string())?;
    let mut models = payload["data"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|item| item["id"].as_str())
        .filter(|id| openai_supports(id, capability))
        .map(|id| ModelOption {
            id: id.to_string(),
            name: id.to_string(),
            description: None,
            prompt_price: None,
            completion_price: None,
        })
        .collect::<Vec<_>>();
    models.sort_by(|left, right| left.id.cmp(&right.id));
    models.dedup_by(|left, right| left.id == right.id);
    Ok(models)
}

fn openai_supports(id: &str, capability: &str) -> bool {
    match capability {
        "transcription" | "stt" => id.contains("transcribe") || id.contains("whisper"),
        "web_search" | "files" => {
            id.starts_with("gpt-4.1")
                || id.starts_with("gpt-5")
                || id.starts_with("o3")
                || id.starts_with("o4")
        }
        "computer" => id.contains("computer-use"),
        "tts" => id.contains("tts"),
        "images" => id.contains("image"),
        "video" => id.contains("sora") || id.contains("video"),
        _ => false,
    }
}

async fn list_anthropic_models(
    client: &Client,
    capability: &str,
    api_key: Option<&str>,
) -> Result<Vec<ModelOption>, String> {
    if !matches!(capability, "web_search" | "computer" | "files") {
        return Ok(Vec::new());
    }
    let api_key = api_key
        .ok_or_else(|| "Collega una API key Anthropic per caricare i modelli.".to_string())?;
    let response = client
        .get("https://api.anthropic.com/v1/models")
        .header("x-api-key", api_key)
        .header("anthropic-version", "2023-06-01")
        .send()
        .await
        .map_err(|error| network_error("Anthropic", error))?;
    let status = response.status();
    let body = response
        .text()
        .await
        .map_err(|error| network_error("Anthropic", error))?;
    if !status.is_success() {
        return Err(api_error("Anthropic", status, &body));
    }
    let payload: Value = serde_json::from_str(&body)
        .map_err(|_| "Anthropic ha restituito un catalogo non valido.".to_string())?;
    Ok(payload["data"].as_array().into_iter().flatten().filter_map(|item| {
        let id = item["id"].as_str()?;
        Some(ModelOption {
            id: id.into(),
            name: item["display_name"].as_str().unwrap_or(id).into(),
            description: Some("Richiede una API key Anthropic; web search verrà collegata in una fase successiva.".into()),
            prompt_price: None,
            completion_price: None,
        })
    }).collect())
}

fn local_model_placeholders(capability: &str) -> Vec<ModelOption> {
    let (id, name, description) = match capability {
        "transcription" | "stt" => (
            "local/whisper",
            "Endpoint Whisper locale",
            "Configura un endpoint locale OpenAI-compatible.",
        ),
        "web_search" => (
            "local/openai-compatible",
            "LLM locale",
            "Richiede endpoint locale e un motore di ricerca separato.",
        ),
        "computer" | "files" => (
            "local/openai-compatible",
            "LLM locale",
            "Capacità registrata ma non abilitata in questo MVP.",
        ),
        "tts" => (
            "local/system-voice",
            "Voce di sistema",
            "Sintesi vocale locale del sistema operativo.",
        ),
        "images" => (
            "local/comfyui",
            "ComfyUI locale",
            "Connettore locale pianificato.",
        ),
        "video" => (
            "local/video",
            "Video locale",
            "Connettore locale pianificato.",
        ),
        _ => return Vec::new(),
    };
    vec![ModelOption {
        id: id.into(),
        name: name.into(),
        description: Some(description.into()),
        prompt_price: Some("0".into()),
        completion_price: Some("0".into()),
    }]
}

fn managed_model_placeholders(capability: &str) -> Vec<ModelOption> {
    match capability {
        "transcription" | "stt" => vec![model(
            "managed/fast-transcription",
            "Onyx Fast Transcription",
        )],
        "web_search" => vec![
            model("managed/fast-search", "Onyx Search Fast"),
            model("managed/deep-search", "Onyx Search Deep"),
        ],
        "tts" => vec![model("managed/voice", "Onyx Voice")],
        "images" => vec![model("managed/image", "Onyx Image")],
        "video" => vec![model("managed/video", "Onyx Video")],
        "computer" => vec![model(
            "managed/computer-use",
            "Onyx Computer Use (prossimamente)",
        )],
        "files" => vec![model("managed/files", "Onyx Files (prossimamente)")],
        _ => Vec::new(),
    }
}

fn model(id: &str, name: &str) -> ModelOption {
    ModelOption {
        id: id.into(),
        name: name.into(),
        description: Some("Incluso nel piano Onyx Managed da €15/mese.".into()),
        prompt_price: None,
        completion_price: None,
    }
}

pub async fn transcribe(
    client: &Client,
    provider: &str,
    api_key: &str,
    model: &str,
    language: Option<&str>,
    request: &TranscriptionRequest,
) -> Result<TranscriptionReply, String> {
    validate_api_key_shape(api_key)?;
    validate_model_id(model)?;
    validate_audio(request)?;
    match provider {
        "openrouter" => transcribe_openrouter(client, api_key, model, language, request).await,
        "openai" => transcribe_openai(client, api_key, model, language, request).await,
        _ => Err("La trascrizione per questo provider non è ancora configurata.".into()),
    }
}

async fn transcribe_openrouter(
    client: &Client,
    api_key: &str,
    model: &str,
    language: Option<&str>,
    request: &TranscriptionRequest,
) -> Result<TranscriptionReply, String> {
    let body = TranscriptionBody {
        model,
        input_audio: InputAudio {
            data: &request.audio_base64,
            format: &request.format,
        },
        language,
        temperature: 0.0,
    };
    let response = client
        .post(format!("{OPENROUTER_BASE}/audio/transcriptions"))
        .bearer_auth(api_key)
        .header("X-OpenRouter-Title", "Onyx")
        .json(&body)
        .send()
        .await
        .map_err(|error| network_error("OpenRouter", error))?;
    parse_transcription("OpenRouter", model, response).await
}

async fn transcribe_openai(
    client: &Client,
    api_key: &str,
    model: &str,
    language: Option<&str>,
    request: &TranscriptionRequest,
) -> Result<TranscriptionReply, String> {
    let audio = STANDARD
        .decode(&request.audio_base64)
        .map_err(|_| "La registrazione audio non è valida.".to_string())?;
    let mime = match request.format.as_str() {
        "wav" => "audio/wav",
        "mp3" => "audio/mpeg",
        "ogg" => "audio/ogg",
        "flac" => "audio/flac",
        "m4a" => "audio/mp4",
        _ => "audio/webm",
    };
    let part = multipart::Part::bytes(audio)
        .file_name(format!("recording.{}", request.format))
        .mime_str(mime)
        .map_err(|_| "Formato audio non valido.".to_string())?;
    let mut form = multipart::Form::new()
        .text("model", model.to_string())
        .part("file", part);
    if let Some(language) = language {
        form = form.text("language", language.to_string());
    }
    let response = client
        .post(format!("{OPENAI_BASE}/audio/transcriptions"))
        .bearer_auth(api_key)
        .multipart(form)
        .send()
        .await
        .map_err(|error| network_error("OpenAI", error))?;
    parse_transcription("OpenAI", model, response).await
}

async fn parse_transcription(
    provider: &str,
    model: &str,
    response: reqwest::Response,
) -> Result<TranscriptionReply, String> {
    let status = response.status();
    let generation_id = response
        .headers()
        .get("x-generation-id")
        .or_else(|| response.headers().get("x-request-id"))
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned);
    let body = response
        .text()
        .await
        .map_err(|error| network_error(provider, error))?;
    if !status.is_success() {
        return Err(api_error(provider, status, &body));
    }
    let payload: TranscriptionResponse = serde_json::from_str(&body)
        .map_err(|_| format!("{provider} ha restituito una trascrizione non valida."))?;
    let text = payload.text.trim().to_string();
    if text.is_empty() {
        return Err("Non sono state rilevate parole comprensibili.".into());
    }
    let seconds = payload
        .usage
        .as_ref()
        .and_then(|usage| usage["seconds"].as_f64());
    let cost = payload
        .usage
        .as_ref()
        .and_then(|usage| usage["cost"].as_f64());
    Ok(TranscriptionReply {
        text,
        model: model.into(),
        generation_id,
        seconds,
        cost,
    })
}

pub async fn search_web(
    client: &Client,
    api_key: &str,
    request: &SearchRequest,
) -> Result<SearchReply, String> {
    validate_api_key_shape(api_key)?;
    validate_model_id(&request.model)?;
    let query = request.query.trim();
    if query.is_empty() || query.chars().count() > 8_000 {
        return Err("La domanda è vuota o troppo lunga.".into());
    }
    match request.provider.as_str() {
        "openrouter" => search_openrouter(client, api_key, request).await,
        "openai" => search_openai(client, api_key, request).await,
        "local" => Err("Per la ricerca con un modello locale devi configurare anche un motore di ricerca.".into()),
        "managed" => Err("Onyx Managed richiede il backend di abbonamento configurato.".into()),
        "anthropic_api" | "claude_subscription_agent_sdk" => Err("La ricerca web Claude sarà collegata tramite il relativo runtime in una fase successiva.".into()),
        _ => Err("Provider di ricerca non supportato.".into()),
    }
}

async fn search_openrouter(
    client: &Client,
    api_key: &str,
    request: &SearchRequest,
) -> Result<SearchReply, String> {
    let mut body = json!({
        "model": request.model,
        "messages": [
            {"role":"system","content":"Sei Onyx, un assistente di ricerca vocale. Rispondi nella lingua della domanda, in modo chiaro e sintetico. Usa la ricerca web e non inventare fonti."},
            {"role":"user","content": request.query}
        ],
        "tools": [{
            "type":"openrouter:web_search",
            "parameters":{"max_results":5,"max_total_results":10}
        }],
        "usage": {"include": true}
    });
    if let Some(effort) = request
        .reasoning
        .as_deref()
        .filter(|value| *value != "none")
    {
        body["reasoning"] = json!({"effort": effort});
    }
    let response = client
        .post(format!("{OPENROUTER_BASE}/chat/completions"))
        .bearer_auth(api_key)
        .header("X-OpenRouter-Title", "Onyx")
        .json(&body)
        .send()
        .await
        .map_err(|error| network_error("OpenRouter", error))?;
    let status = response.status();
    let raw = response
        .text()
        .await
        .map_err(|error| network_error("OpenRouter", error))?;
    if !status.is_success() {
        return Err(api_error("OpenRouter", status, &raw));
    }
    let payload: Value = serde_json::from_str(&raw)
        .map_err(|_| "OpenRouter ha restituito una risposta non valida.".to_string())?;
    let message = &payload["choices"][0]["message"];
    let answer = message["content"]
        .as_str()
        .unwrap_or_default()
        .trim()
        .to_string();
    if answer.is_empty() {
        return Err("Il modello non ha prodotto una risposta.".into());
    }
    let sources = parse_openrouter_sources(message);
    let usage = SearchUsage {
        input_tokens: payload["usage"]["input_tokens"]
            .as_u64()
            .or_else(|| payload["usage"]["prompt_tokens"].as_u64()),
        output_tokens: payload["usage"]["output_tokens"]
            .as_u64()
            .or_else(|| payload["usage"]["completion_tokens"].as_u64()),
        cost: payload["usage"]["cost"].as_f64(),
    };
    Ok(SearchReply {
        answer,
        model: request.model.clone(),
        sources,
        usage,
    })
}

fn parse_openrouter_sources(message: &Value) -> Vec<SearchSource> {
    let mut seen = HashSet::new();
    message["annotations"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|annotation| {
            let citation = annotation.get("url_citation").unwrap_or(annotation);
            let url = citation["url"].as_str()?.to_string();
            if !seen.insert(url.clone()) {
                return None;
            }
            Some(SearchSource {
                title: citation["title"].as_str().unwrap_or("Fonte").to_string(),
                url,
                snippet: citation["content"]
                    .as_str()
                    .map(|value| truncate(value, 220)),
            })
        })
        .collect()
}

async fn search_openai(
    client: &Client,
    api_key: &str,
    request: &SearchRequest,
) -> Result<SearchReply, String> {
    if !openai_supports(&request.model, "web_search") {
        return Err(
            "Il modello OpenAI selezionato non supporta la ricerca web Responses API.".into(),
        );
    }
    let mut body = json!({
        "model": request.model,
        "tools": [{"type":"web_search","search_context_size":"medium"}],
        "tool_choice": "auto",
        "include": ["web_search_call.action.sources"],
        "instructions": "Sei Onyx, un assistente di ricerca vocale. Rispondi nella lingua della domanda in modo chiaro e sintetico. Non inventare fonti.",
        "input": request.query
    });
    if let Some(effort) = request
        .reasoning
        .as_deref()
        .filter(|value| *value != "none" && supports_reasoning(&request.model))
    {
        body["reasoning"] = json!({"effort": effort});
    }
    let response = client
        .post(format!("{OPENAI_BASE}/responses"))
        .bearer_auth(api_key)
        .json(&body)
        .send()
        .await
        .map_err(|error| network_error("OpenAI", error))?;
    let status = response.status();
    let raw = response
        .text()
        .await
        .map_err(|error| network_error("OpenAI", error))?;
    if !status.is_success() {
        return Err(api_error("OpenAI", status, &raw));
    }
    let payload: Value = serde_json::from_str(&raw)
        .map_err(|_| "OpenAI ha restituito una risposta non valida.".to_string())?;
    let (answer, mut sources) = parse_openai_response(&payload);
    if answer.is_empty() {
        return Err("Il modello non ha prodotto una risposta.".into());
    }
    append_openai_search_sources(&payload, &mut sources);
    let usage = SearchUsage {
        input_tokens: payload["usage"]["input_tokens"].as_u64(),
        output_tokens: payload["usage"]["output_tokens"].as_u64(),
        cost: None,
    };
    Ok(SearchReply {
        answer,
        model: request.model.clone(),
        sources,
        usage,
    })
}

fn supports_reasoning(model: &str) -> bool {
    let id = model.rsplit('/').next().unwrap_or(model);
    id.starts_with("gpt-5")
        || id.starts_with("o3")
        || id.starts_with("o4")
        || model == "openrouter/auto"
        || model.contains("claude")
        || model.contains("gemini")
}

fn parse_openai_response(payload: &Value) -> (String, Vec<SearchSource>) {
    let mut answer = String::new();
    let mut sources = Vec::new();
    let mut seen = HashSet::new();
    for item in payload["output"].as_array().into_iter().flatten() {
        if item["type"].as_str() != Some("message") {
            continue;
        }
        for content in item["content"].as_array().into_iter().flatten() {
            if content["type"].as_str() != Some("output_text") {
                continue;
            }
            if let Some(text) = content["text"].as_str() {
                answer.push_str(text);
            }
            for annotation in content["annotations"].as_array().into_iter().flatten() {
                if annotation["type"].as_str() != Some("url_citation") {
                    continue;
                }
                let Some(url) = annotation["url"].as_str() else {
                    continue;
                };
                if seen.insert(url.to_string()) {
                    sources.push(SearchSource {
                        title: annotation["title"].as_str().unwrap_or("Fonte").into(),
                        url: url.into(),
                        snippet: None,
                    });
                }
            }
        }
    }
    (answer.trim().to_string(), sources)
}

fn append_openai_search_sources(payload: &Value, sources: &mut Vec<SearchSource>) {
    let mut seen = sources
        .iter()
        .map(|item| item.url.clone())
        .collect::<HashSet<_>>();
    for item in payload["output"].as_array().into_iter().flatten() {
        for source in item["action"]["sources"].as_array().into_iter().flatten() {
            let Some(url) = source["url"].as_str() else {
                continue;
            };
            if seen.insert(url.into()) {
                sources.push(SearchSource {
                    title: source["title"].as_str().unwrap_or("Fonte").into(),
                    url: url.into(),
                    snippet: None,
                });
            }
        }
    }
}

pub fn validate_model_id(model: &str) -> Result<(), String> {
    let value = model.trim();
    if value.is_empty()
        || value.len() > 180
        || value.contains("://")
        || value.contains("//")
        || !value.bytes().all(|byte| {
            byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b':' | b'/' | b'-' | b'~')
        })
    {
        return Err("Identificatore modello non valido.".into());
    }
    Ok(())
}

pub(crate) fn validate_api_key_shape(api_key: &str) -> Result<(), String> {
    if api_key.trim() != api_key
        || api_key.len() < 12
        || api_key.len() > 512
        || api_key.chars().any(char::is_whitespace)
    {
        return Err("Chiave API non valida.".into());
    }
    Ok(())
}

fn validate_audio(request: &TranscriptionRequest) -> Result<(), String> {
    if request.audio_base64.is_empty() || request.audio_base64.len() > MAX_AUDIO_BASE64_BYTES {
        return Err("La registrazione è vuota o troppo grande.".into());
    }
    if !matches!(
        request.format.as_str(),
        "wav" | "mp3" | "ogg" | "flac" | "m4a" | "webm"
    ) {
        return Err("Formato audio non supportato.".into());
    }
    STANDARD
        .decode(&request.audio_base64)
        .map_err(|_| "La registrazione audio non è valida.".to_string())?;
    Ok(())
}

pub(crate) fn network_error(provider: &str, error: reqwest::Error) -> String {
    if error.is_timeout() {
        format!("{provider} non ha risposto in tempo. Riprova.")
    } else if error.is_connect() {
        format!("Non riesco a raggiungere {provider}. Controlla la connessione Internet.")
    } else {
        format!("Richiesta {provider} non riuscita: {error}")
    }
}

pub(crate) fn api_error(provider: &str, status: StatusCode, body: &str) -> String {
    let detail = serde_json::from_str::<Value>(body).ok().and_then(|value| {
        value
            .pointer("/error/message")
            .or_else(|| value.get("message"))
            .and_then(Value::as_str)
            .and_then(safe_provider_detail)
    });
    let prefix = match status {
        StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => {
            format!("Accesso {provider} non valido. Ricollega l'account")
        }
        StatusCode::PAYMENT_REQUIRED => format!("Credito {provider} insufficiente"),
        StatusCode::TOO_MANY_REQUESTS => format!("Limite {provider} raggiunto. Attendi e riprova"),
        _ if status.is_server_error() => format!("{provider} è temporaneamente non disponibile"),
        _ => format!("{provider} ha rifiutato la richiesta"),
    };
    match detail {
        Some(detail) if !detail.is_empty() => format!("{prefix}: {detail}"),
        _ => format!("{prefix} (HTTP {}).", status.as_u16()),
    }
}

/// Provider error bodies are untrusted. Do not surface text that looks like it
/// may contain credentials copied into an upstream error message.
fn safe_provider_detail(message: &str) -> Option<String> {
    let lowered = message.to_ascii_lowercase();
    let looks_sensitive = [
        "sk-",
        "bearer ",
        "api_key",
        "api-key",
        "authorization:",
        "refresh_token",
        "access_token",
    ]
    .iter()
    .any(|marker| lowered.contains(marker));

    if looks_sensitive {
        None
    } else {
        let detail = truncate(message.trim(), 280);
        (!detail.is_empty()).then_some(detail)
    }
}

fn truncate(value: &str, limit: usize) -> String {
    value.chars().take(limit).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn model_id_accepts_openai_and_router_ids() {
        assert!(validate_model_id("openai/whisper-large-v3").is_ok());
        assert!(validate_model_id("gpt-5.6").is_ok());
        assert!(validate_model_id("https://example.test/model").is_err());
        assert!(validate_model_id("bad model").is_err());
    }

    #[test]
    fn api_error_never_echoes_unstructured_secret() {
        let secret = "sk-super-secret";
        let message = api_error("OpenRouter", StatusCode::UNAUTHORIZED, secret);
        assert!(!message.contains(secret));
    }

    #[test]
    fn api_error_never_echoes_structured_secret() {
        let secret = "sk-super-secret";
        let body = format!(r#"{{"error":{{"message":"invalid key {secret}"}}}}"#);
        let message = api_error("OpenRouter", StatusCode::UNAUTHORIZED, &body);
        assert!(!message.contains(secret));
    }
}
