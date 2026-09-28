use crate::{
    db::Database,
    error::AppError,
    local_asr,
    models::{RecordingConfig, TranscriptSegment},
    secrets,
};
use std::path::{Path, PathBuf};

pub async fn test_provider(
    config: &RecordingConfig,
    db: &Database,
    app_data: &Path,
) -> Result<(), AppError> {
    match config.provider.as_str() {
        "openai" => {
            let key = secrets::get(db, "asr-openai").or_else(|_| secrets::get(db, "llm-openai"))?;
            let model = config.model.as_deref().unwrap_or("gpt-4o-mini-transcribe");
            let base = config
                .base_url
                .as_deref()
                .unwrap_or("https://api.openai.com/v1")
                .trim_end_matches('/');
            let response = reqwest::Client::new()
                .get(format!("{base}/models/{model}"))
                .bearer_auth(key)
                .send()
                .await
                .map_err(|e| AppError::new("asr.network", e.to_string()).retryable())?;
            if response.status().is_success() {
                Ok(())
            } else {
                Err(AppError::new(
                    "asr.http",
                    format!("连接测试失败：{}", response.status()),
                ))
            }
        }
        "qwen3-asr" | "faster-whisper" => {
            let model = local_model_path(config)?;
            let provider = config.provider.clone();
            let language = config.language.clone();
            let compute_device = config.compute_device.clone();
            let app_data = app_data.to_owned();
            tokio::task::spawn_blocking(move || {
                local_asr::validate(
                    &app_data,
                    &model,
                    &provider,
                    language.as_deref(),
                    &compute_device,
                )
            })
            .await
            .map_err(|e| AppError::new("asr.runtime", e.to_string()))?
        }
        "sherpa-streaming" => {
            let model = local_model_path(config)?;
            let app_data = app_data.to_owned();
            let compute_device = config.compute_device.clone();
            tokio::task::spawn_blocking(move || {
                local_asr::validate_streaming(&app_data, &model, &compute_device)
            })
            .await
            .map_err(|e| AppError::new("asr.runtime", e.to_string()))?
        }
        "xunfei" => {
            secrets::get(db, "asr-xunfei-app-id")?;
            secrets::get(db, "asr-xunfei-api-key")?;
            secrets::get(db, "asr-xunfei-api-secret")?;
            Ok(())
        }
        "tencent" => crate::streaming_asr::test_tencent_connection(db).await,
        _ => Err(AppError::new("asr.provider", "不支持的 ASR 提供方")),
    }
}

pub async fn transcribe_samples(
    samples: &[f32],
    config: &RecordingConfig,
    _db: &Database,
    app_data: &Path,
) -> Result<TranscriptSegment, AppError> {
    match config.provider.as_str() {
        "qwen3-asr" | "faster-whisper" => local_from_samples(samples, config, app_data).await,
        _ => Err(AppError::new("asr.provider", "不支持的 ASR 提供方")),
    }
}

pub fn warmup_local(
    app_data: &Path,
    model: &Path,
    provider: &str,
    language: Option<&str>,
    compute_device: &str,
) -> Result<(), AppError> {
    local_asr::validate(app_data, model, provider, language, compute_device)
}

async fn local_from_samples(
    samples: &[f32],
    config: &RecordingConfig,
    app_data: &Path,
) -> Result<TranscriptSegment, AppError> {
    let model = local_model_path(config)?;
    let provider = config.provider.clone();
    let language = config.language.clone();
    let samples = samples.to_vec();
    let app_data = app_data.to_owned();
    let compute_device = config.compute_device.clone();
    let text = tokio::task::spawn_blocking(move || {
        local_asr::transcribe_samples(
            &app_data,
            &model,
            &provider,
            language.as_deref(),
            &samples,
            16_000,
            &compute_device,
        )
    })
    .await
    .map_err(|e| AppError::new("asr.runtime", e.to_string()))??;
    Ok(local_segment(text))
}

fn local_segment(text: String) -> TranscriptSegment {
    TranscriptSegment {
        text,
        stable: true,
        start_ms: 0,
        end_ms: 0,
        confidence: None,
    }
}

fn local_model_path(config: &RecordingConfig) -> Result<PathBuf, AppError> {
    config
        .model
        .as_deref()
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .ok_or_else(|| AppError::new("asr.model_missing", "请先下载并选择本地模型"))
}
