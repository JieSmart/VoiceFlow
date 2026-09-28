use crate::{db::Database, error::AppError, models::LlmRequest, secrets};
use futures_util::StreamExt;
use tauri::{AppHandle, Emitter};

fn endpoint(base: Option<&str>, suffix: &str, default: &str) -> String {
    let Some(base) = base.map(str::trim).filter(|value| !value.is_empty()) else {
        return default.into();
    };
    if base.ends_with(suffix) {
        return base.to_owned();
    }
    if suffix.starts_with("/v1") && base.ends_with("/v1") {
        return format!("{}{}", base.trim_end_matches('/'), &suffix[3..]);
    }
    format!("{}{}", base.trim_end_matches('/'), suffix)
}

pub async fn stream(
    app: AppHandle,
    request: LlmRequest,
    db: &Database,
) -> Result<String, AppError> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(120))
        .build()
        .map_err(|e| AppError::new("llm.client", e.to_string()))?;
    let req = match request.provider.as_str() {
        "openai" => {
            let key = secrets::get(db, "llm-openai")?;
            let url = endpoint(
                request.base_url.as_deref(),
                "/chat/completions",
                "https://api.openai.com/v1/chat/completions",
            );
            let mut body = serde_json::json!({
                "model": request.model,
                "messages": [
                    {"role": "system", "content": request.instructions},
                    {"role": "user", "content": request.input}
                ],
                "max_tokens": request.max_output_tokens,
                "stream": true
            });
            apply_openai_thinking(&mut body, request.thinking);
            client.post(url).bearer_auth(key).json(&body)
        }
        "claude" => {
            let key = secrets::get(db, "llm-claude")?;
            let url = endpoint(
                request.base_url.as_deref(),
                "/v1/messages",
                "https://api.anthropic.com/v1/messages",
            );
            let mut body = serde_json::json!({
                "model": request.model,
                "system": request.instructions,
                "messages": [{"role": "user", "content": request.input}],
                "max_tokens": request.max_output_tokens,
                "stream": true
            });
            apply_claude_thinking(&mut body, request.thinking);
            client
                .post(url)
                .header("x-api-key", key)
                .header("anthropic-version", "2023-06-01")
                .json(&body)
        }
        _ => return Err(AppError::new("llm.provider", "不支持的 LLM 提供方")),
    };
    let response = req
        .send()
        .await
        .map_err(|e| AppError::new("llm.network", e.to_string()).retryable())?;
    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(AppError::new("llm.http", format!("{status}: {body}")));
    }
    let mut bytes = response.bytes_stream();
    let mut pending = String::new();
    let mut output = String::new();
    while let Some(chunk) = bytes.next().await {
        pending.push_str(&String::from_utf8_lossy(
            &chunk.map_err(|e| AppError::new("llm.stream", e.to_string()).retryable())?,
        ));
        while let Some(pos) = pending.find("\n\n") {
            let block: String = pending.drain(..pos + 2).collect();
            for line in block.lines().filter_map(|l| l.strip_prefix("data: ")) {
                if line == "[DONE]" {
                    continue;
                }
                let Ok(value) = serde_json::from_str::<serde_json::Value>(line) else {
                    continue;
                };
                let delta = if request.provider == "openai"
                    && value["choices"][0]["delta"]["content"].is_string()
                {
                    value["choices"][0]["delta"]["content"].as_str()
                } else if request.provider == "claude"
                    && value["type"] == "content_block_delta"
                    && value["delta"]["type"] == "text_delta"
                {
                    value["delta"]["text"].as_str()
                } else {
                    None
                };
                if let Some(delta) = delta {
                    output.push_str(delta);
                    let _ = app.emit("llm-text-delta", serde_json::json!({"delta":delta,"text":output,"provider":request.provider}));
                }
                if matches!(value["type"].as_str(), Some("response.failed" | "error"))
                    || value["error"].is_object()
                {
                    return Err(AppError::new("llm.remote", value.to_string()));
                }
            }
        }
    }
    let _ = app.emit(
        "llm-complete",
        serde_json::json!({"text":output,"provider":request.provider}),
    );
    Ok(output)
}

pub async fn test(
    provider: &str,
    model: &str,
    base_url: Option<&str>,
    db: &Database,
) -> Result<(), AppError> {
    let req = LlmRequest {
        provider: provider.into(),
        model: model.into(),
        base_url: base_url.map(str::to_owned),
        instructions: "只回复 OK".into(),
        input: "连接测试".into(),
        max_output_tokens: 8,
        thinking: false,
    };
    // Test uses the same protocol, but without exposing an AppHandle is handled by a minimal non-stream request.
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(20))
        .build()
        .map_err(|e| AppError::new("llm.client", e.to_string()))?;
    let response = if provider == "openai" {
        let url = endpoint(
            base_url,
            "/chat/completions",
            "https://api.openai.com/v1/chat/completions",
        );
        client.post(url).bearer_auth(secrets::get(db, "llm-openai")?).json(&serde_json::json!({"model":model,"messages":[{"role":"system","content":req.instructions},{"role":"user","content":req.input}],"max_tokens":8})).send().await
    } else if provider == "claude" {
        let url = endpoint(
            base_url,
            "/v1/messages",
            "https://api.anthropic.com/v1/messages",
        );
        client.post(url).header("x-api-key", secrets::get(db, "llm-claude")?).header("anthropic-version", "2023-06-01").json(&serde_json::json!({"model":model,"system":req.instructions,"messages":[{"role":"user","content":req.input}],"max_tokens":8})).send().await
    } else {
        return Err(AppError::new("llm.provider", "不支持的 LLM 提供方"));
    };
    let response = response.map_err(|e| AppError::new("llm.network", e.to_string()).retryable())?;
    if response.status().is_success() {
        Ok(())
    } else {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        Err(AppError::new(
            "llm.http",
            format!("连接测试失败：{status}: {body}"),
        ))
    }
}

fn apply_openai_thinking(body: &mut serde_json::Value, thinking: bool) {
    let Some(object) = body.as_object_mut() else {
        return;
    };
    if thinking {
        object.insert(
            "reasoning_effort".into(),
            serde_json::Value::String("medium".into()),
        );
        object.remove("chat_template_kwargs");
    } else {
        object.insert(
            "reasoning_effort".into(),
            serde_json::Value::String("none".into()),
        );
        object.insert(
            "chat_template_kwargs".into(),
            serde_json::json!({ "enable_thinking": false }),
        );
    }
}

fn apply_claude_thinking(body: &mut serde_json::Value, thinking: bool) {
    let Some(object) = body.as_object_mut() else {
        return;
    };
    if thinking {
        let max_tokens = object
            .get("max_tokens")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(2048)
            .max(1536);
        object.insert("max_tokens".into(), serde_json::json!(max_tokens));
        object.insert(
            "thinking".into(),
            serde_json::json!({ "type": "enabled", "budget_tokens": 1024 }),
        );
    } else {
        object.insert("thinking".into(), serde_json::json!({ "type": "disabled" }));
    }
}

pub async fn list_models(
    provider: &str,
    base_url: Option<&str>,
    db: &Database,
) -> Result<Vec<serde_json::Value>, AppError> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(20))
        .build()
        .map_err(|e| AppError::new("llm.client", e.to_string()))?;
    let request = match provider {
        "openai" => {
            let url = endpoint(base_url, "/models", "https://api.openai.com/v1/models");
            client.get(url).bearer_auth(secrets::get(db, "llm-openai")?)
        }
        "claude" => {
            let url = endpoint(
                base_url,
                "/v1/models",
                "https://api.anthropic.com/v1/models",
            );
            client
                .get(url)
                .header("x-api-key", secrets::get(db, "llm-claude")?)
                .header("anthropic-version", "2023-06-01")
        }
        _ => return Err(AppError::new("llm.provider", "不支持的 LLM 提供方")),
    };
    let response = request
        .send()
        .await
        .map_err(|e| AppError::new("llm.network", e.to_string()).retryable())?;
    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(AppError::new(
            "llm.http",
            format!("获取模型列表失败：{status}: {body}"),
        ));
    }
    let value: serde_json::Value = response
        .json()
        .await
        .map_err(|e| AppError::new("llm.response", e.to_string()))?;
    let items = value
        .get("data")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    Ok(items)
}

#[cfg(test)]
mod tests {
    use super::{apply_claude_thinking, apply_openai_thinking, endpoint};

    #[test]
    fn builds_openai_compatible_endpoints_without_duplicate_paths() {
        assert_eq!(
            endpoint(
                Some("https://api.openai.com/v1"),
                "/chat/completions",
                "unused"
            ),
            "https://api.openai.com/v1/chat/completions"
        );
        assert_eq!(
            endpoint(
                Some("https://example.com/v1/chat/completions"),
                "/chat/completions",
                "unused"
            ),
            "https://example.com/v1/chat/completions"
        );
        assert_eq!(
            endpoint(Some("https://api.openai.com/v1"), "/models", "unused"),
            "https://api.openai.com/v1/models"
        );
    }

    #[test]
    fn openai_thinking_off_disables_reasoning_effort() {
        let mut body = serde_json::json!({"model": "gpt-5-mini"});
        apply_openai_thinking(&mut body, false);
        assert_eq!(body["reasoning_effort"], "none");
        assert_eq!(body["chat_template_kwargs"]["enable_thinking"], false);
        apply_openai_thinking(&mut body, true);
        assert_eq!(body["reasoning_effort"], "medium");
        assert!(body.get("chat_template_kwargs").is_none());
    }

    #[test]
    fn claude_thinking_off_sends_disabled_block() {
        let mut body = serde_json::json!({"model": "claude-sonnet-4-5"});
        apply_claude_thinking(&mut body, false);
        assert_eq!(body["thinking"]["type"], "disabled");
        apply_claude_thinking(&mut body, true);
        assert_eq!(body["thinking"]["type"], "enabled");
        assert_eq!(body["thinking"]["budget_tokens"], 1024);
    }

    #[test]
    fn builds_claude_endpoints_without_duplicate_v1() {
        assert_eq!(
            endpoint(
                Some("https://api.anthropic.com/v1"),
                "/v1/messages",
                "unused"
            ),
            "https://api.anthropic.com/v1/messages"
        );
        assert_eq!(
            endpoint(Some("https://api.anthropic.com/v1"), "/v1/models", "unused"),
            "https://api.anthropic.com/v1/models"
        );
    }
}
