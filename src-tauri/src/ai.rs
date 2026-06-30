use reqwest::Client;
use serde_json::json;
use tauri::{AppHandle, Emitter};

use crate::types::AiStreamEvent;

const ANTHROPIC_API_URL: &str = "https://api.anthropic.com/v1/messages";
const OPENAI_API_URL: &str = "https://api.openai.com/v1/chat/completions";
const OLLAMA_DEFAULT_URL: &str = "http://localhost:11434";

pub fn ai_request(
    app: AppHandle,
    provider: String,
    api_key: String,
    model: String,
    system_prompt: String,
    user_message: String,
    request_id: String,
    base_url: Option<String>,
) {
    std::thread::spawn(move || {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            // Handle all API keys as optional; ollama and v1 completions doesnt always require it.
            let key_opt = if api_key.is_empty() { None } else { Some(api_key.as_str()) };
            let result = match provider.as_str() {
                "openai" => {
                    stream_openai(
                        &app,
                        OPENAI_API_URL,
                        Some(&api_key),
                        &model,
                        &system_prompt,
                        &user_message,
                        &request_id,
                    )
                    .await
                }
                "ollama" => {
                    let url = base_url.as_deref().unwrap_or(OLLAMA_DEFAULT_URL);
                    let url = format!("{}/v1/chat/completions", url.trim_end_matches('/'));
                    stream_openai(
                        &app,
                        &url,
                        key_opt,
                        &model,
                        &system_prompt,
                        &user_message,
                        &request_id,
                    )
                    .await
                }
                "openai_compatible" => {
                    let url = base_url.as_deref().unwrap_or("");
                    if url.is_empty() {
                        Err("No base URL configured for OpenAI Compatible provider".to_string())
                    } else {
                        let url = format!("{}/v1/chat/completions", normalize_openai_base(url));
                        stream_openai(
                            &app,
                            &url,
                            key_opt,
                            &model,
                            &system_prompt,
                            &user_message,
                            &request_id,
                        )
                        .await
                    }
                }
                _ => {
                    stream_anthropic(
                        &app,
                        &api_key,
                        &model,
                        &system_prompt,
                        &user_message,
                        &request_id,
                    )
                    .await
                }
            };
            if let Err(e) = result {
                let _ = app.emit(
                    "ai-stream",
                    AiStreamEvent {
                        event_type: "error".to_string(),
                        text: None,
                        error: Some(e),
                    },
                );
            }
        });
    });
}

/// Normalize an OpenAI-compatible base URL: drop a trailing slash and a trailing `/v1`,
/// so both `https://host` and `https://host/v1` work (we append `/v1/chat/completions`).
fn normalize_openai_base(base: &str) -> String {
    let b = base.trim().trim_end_matches('/');
    b.strip_suffix("/v1").unwrap_or(b).trim_end_matches('/').to_string()
}

async fn stream_anthropic(
    app: &AppHandle,
    api_key: &str,
    model: &str,
    system_prompt: &str,
    user_message: &str,
    _request_id: &str,
) -> Result<(), String> {
    let client = Client::new();

    let body = json!({
        "model": model,
        "max_tokens": 4096,
        "stream": true,
        "system": system_prompt,
        "messages": [
            {
                "role": "user",
                "content": user_message
            }
        ]
    });

    let response = client
        .post(ANTHROPIC_API_URL)
        .header("x-api-key", api_key)
        .header("anthropic-version", "2023-06-01")
        .header("content-type", "application/json")
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("Request failed: {}", e))?;

    if !response.status().is_success() {
        let status = response.status();
        let body_text = response.text().await.unwrap_or_default();
        return Err(format!("API error {}: {}", status, body_text));
    }

    // Parse SSE stream
    use futures::StreamExt;
    let mut stream = response.bytes_stream();
    let mut buffer = String::new();

    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| format!("Stream error: {}", e))?;
        buffer.push_str(&String::from_utf8_lossy(&chunk));

        // Process complete SSE events from buffer
        while let Some(event_end) = buffer.find("\n\n") {
            let event_str = buffer[..event_end].to_string();
            buffer = buffer[event_end + 2..].to_string();

            for line in event_str.lines() {
                if let Some(data) = line.strip_prefix("data: ") {
                    if data == "[DONE]" {
                        let _ = app.emit(
                            "ai-stream",
                            AiStreamEvent {
                                event_type: "done".to_string(),
                                text: None,
                                error: None,
                            },
                        );
                        return Ok(());
                    }

                    if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(data) {
                        let event_type = parsed["type"].as_str().unwrap_or("");

                        match event_type {
                            "content_block_delta" => {
                                if let Some(text) = parsed["delta"]["text"].as_str() {
                                    let _ = app.emit(
                                        "ai-stream",
                                        AiStreamEvent {
                                            event_type: "text".to_string(),
                                            text: Some(text.to_string()),
                                            error: None,
                                        },
                                    );
                                }
                            }
                            "message_stop" => {
                                let _ = app.emit(
                                    "ai-stream",
                                    AiStreamEvent {
                                        event_type: "done".to_string(),
                                        text: None,
                                        error: None,
                                    },
                                );
                                return Ok(());
                            }
                            "error" => {
                                let msg = parsed["error"]["message"]
                                    .as_str()
                                    .unwrap_or("Unknown API error");
                                let _ = app.emit(
                                    "ai-stream",
                                    AiStreamEvent {
                                        event_type: "error".to_string(),
                                        text: None,
                                        error: Some(msg.to_string()),
                                    },
                                );
                                return Err(msg.to_string());
                            }
                            _ => {}
                        }
                    }
                }
            }
        }
    }

    let _ = app.emit(
        "ai-stream",
        AiStreamEvent {
            event_type: "done".to_string(),
            text: None,
            error: None,
        },
    );

    Ok(())
}

async fn stream_openai(
    app: &AppHandle,
    url: &str,
    api_key: Option<&str>,
    model: &str,
    system_prompt: &str,
    user_message: &str,
    _request_id: &str,
) -> Result<(), String> {
    let client = Client::new();

    let is_gpt5 = model.starts_with("gpt-5");
    let token_key = if is_gpt5 {
        "max_completion_tokens"
    } else {
        "max_tokens"
    };

    let mut body = json!({
        "model": model,
        "stream": true,
        token_key: 4096,
        "messages": [
            {
                "role": "system",
                "content": system_prompt
            },
            {
                "role": "user",
                "content": user_message
            }
        ]
    });

    // GPT-5 models don't support temperature
    if !is_gpt5 {
        body["temperature"] = json!(0.7);
    }

    let mut req = client
        .post(url)
        .header("content-type", "application/json");

    if let Some(key) = api_key {
        req = req.header("Authorization", format!("Bearer {}", key));
    }

    let response = req
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("Request failed: {}", e))?;

    if !response.status().is_success() {
        let status = response.status();
        let body_text = response.text().await.unwrap_or_default();
        return Err(format!("API error {}: {}", status, body_text));
    }

    use futures::StreamExt;
    let mut stream = response.bytes_stream();
    let mut buffer = String::new();

    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| format!("Stream error: {}", e))?;
        buffer.push_str(&String::from_utf8_lossy(&chunk));

        while let Some(event_end) = buffer.find("\n\n") {
            let event_str = buffer[..event_end].to_string();
            buffer = buffer[event_end + 2..].to_string();

            for line in event_str.lines() {
                if let Some(data) = line.strip_prefix("data: ") {
                    if data == "[DONE]" {
                        let _ = app.emit(
                            "ai-stream",
                            AiStreamEvent {
                                event_type: "done".to_string(),
                                text: None,
                                error: None,
                            },
                        );
                        return Ok(());
                    }

                    if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(data) {
                        // OpenAI streaming: choices[0].delta.content
                        if let Some(content) = parsed["choices"][0]["delta"]["content"].as_str() {
                            if !content.is_empty() {
                                let _ = app.emit(
                                    "ai-stream",
                                    AiStreamEvent {
                                        event_type: "text".to_string(),
                                        text: Some(content.to_string()),
                                        error: None,
                                    },
                                );
                            }
                        }

                        // Check finish_reason
                        if let Some(reason) = parsed["choices"][0]["finish_reason"].as_str() {
                            if reason == "stop" || reason == "length" {
                                let _ = app.emit(
                                    "ai-stream",
                                    AiStreamEvent {
                                        event_type: "done".to_string(),
                                        text: None,
                                        error: None,
                                    },
                                );
                                return Ok(());
                            }
                        }

                        // Check for error in stream
                        if let Some(err) = parsed["error"]["message"].as_str() {
                            let _ = app.emit(
                                "ai-stream",
                                AiStreamEvent {
                                    event_type: "error".to_string(),
                                    text: None,
                                    error: Some(err.to_string()),
                                },
                            );
                            return Err(err.to_string());
                        }
                    }
                }
            }
        }
    }

    let _ = app.emit(
        "ai-stream",
        AiStreamEvent {
            event_type: "done".to_string(),
            text: None,
            error: None,
        },
    );

    Ok(())
}

pub async fn test_connection(
    provider: &str,
    api_key: &str,
    model: &str,
    base_url: Option<&str>,
) -> Result<String, String> {
    let key_opt = if api_key.is_empty() { None } else { Some(api_key) };
    match provider {
        "openai" => test_openai(OPENAI_API_URL, Some(api_key), model).await,
        "ollama" => {
            let url = base_url.unwrap_or(OLLAMA_DEFAULT_URL);
            let url = format!("{}/v1/chat/completions", url.trim_end_matches('/'));
            test_openai(&url, key_opt, model).await
        }
        "openai_compatible" => {
            let url = base_url.unwrap_or("");
            if url.is_empty() {
                return Err("No base URL configured for OpenAI Compatible provider".to_string());
            }
            let url = format!("{}/v1/chat/completions", normalize_openai_base(url));
            test_openai(&url, key_opt, model).await
        }
        _ => test_anthropic(api_key, model).await,
    }
}

async fn test_anthropic(api_key: &str, model: &str) -> Result<String, String> {
    let client = Client::new();

    let body = json!({
        "model": model,
        "max_tokens": 10,
        "messages": [
            {
                "role": "user",
                "content": "Hi"
            }
        ]
    });

    let response = client
        .post(ANTHROPIC_API_URL)
        .header("x-api-key", api_key)
        .header("anthropic-version", "2023-06-01")
        .header("content-type", "application/json")
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("Connection failed: {}", e))?;

    if response.status().is_success() {
        Ok("Connection successful".to_string())
    } else {
        let status = response.status();
        let body_text = response.text().await.unwrap_or_default();
        Err(format!("API error {}: {}", status, body_text))
    }
}

async fn test_openai(url: &str, api_key: Option<&str>, model: &str) -> Result<String, String> {
	let client = Client::new();
	let is_gpt5 = model.starts_with("gpt-5");
	let token_key = if is_gpt5 { "max_completion_tokens" } else { "max_tokens" };

	let body = json!({
		"model": model,
		token_key: 10,
		"messages": [
            {
                "role": "user",
                "content": "Hi"
            }
        ]
    });

    let mut req = client
        .post(url)
        .header("content-type", "application/json");

    if let Some(key) = api_key {
        req = req.header("Authorization", format!("Bearer {}", key));
    }

    let response = req
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("Connection failed: {}", e))?;

    if response.status().is_success() {
        Ok("Connection successful".to_string())
    } else {
        let status = response.status();
        let body_text = response.text().await.unwrap_or_default();
        Err(format!("API error {}: {}", status, body_text))
    }
}
