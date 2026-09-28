use crate::{db::Database, error::AppError, models::RecordingConfig, secrets};
use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use futures_util::{SinkExt, StreamExt};
use hmac::{Hmac, Mac};
use parking_lot::Mutex;
use serde_json::{json, Value};
use sha1::Sha1;
use sha2::Sha256;
use std::{
    collections::{BTreeMap, HashMap},
    path::PathBuf,
    sync::Arc,
    time::{Duration, SystemTime},
};
use tauri::{AppHandle, Emitter};
use tokio::sync::mpsc;
use tokio_tungstenite::{connect_async, tungstenite::client::IntoClientRequest};

#[derive(Debug)]
pub struct AudioChunk {
    pub samples: Vec<f32>,
    pub sample_rate: u32,
    pub channels: u16,
}

#[derive(Debug)]
pub enum StreamMessage {
    Audio(AudioChunk),
    Finish,
    Cancel,
}

pub struct StreamHandle {
    pub sender: mpsc::Sender<StreamMessage>,
    join: tauri::async_runtime::JoinHandle<()>,
}

#[derive(Clone, Default)]
pub struct StreamingManager(pub Arc<Mutex<HashMap<String, StreamHandle>>>);

impl StreamingManager {
    pub fn contains(&self, session_id: &str) -> bool {
        self.0.lock().contains_key(session_id)
    }

    pub fn start(
        &self,
        app: AppHandle,
        session_id: String,
        config: RecordingConfig,
        db: Database,
        data_root: PathBuf,
    ) -> Result<Option<mpsc::Sender<StreamMessage>>, AppError> {
        if !is_streaming_provider(&config.provider) {
            return Ok(None);
        }
        let (sender, receiver) = mpsc::channel(96);
        let task_sender = sender.clone();
        let task_session = session_id.clone();
        let join = tauri::async_runtime::spawn(async move {
            let result = match config.provider.as_str() {
                "openai" => run_openai(app.clone(), &task_session, config, db, receiver).await,
                "xunfei" => run_xunfei(app.clone(), &task_session, db.clone(), receiver).await,
                "tencent" => run_tencent(app.clone(), &task_session, db, receiver).await,
                "sherpa-streaming" => {
                    run_local(app.clone(), &task_session, config, data_root, receiver).await
                }
                _ => unreachable!(),
            };
            if let Err(error) = result {
                let _ = app.emit(
                    "session-error",
                    json!({
                        "sessionId": task_session,
                        "code": error.code,
                        "message": error.message,
                        "retryable": error.retryable,
                        "fallback": error.fallback,
                    }),
                );
            }
        });
        self.0.lock().insert(
            session_id,
            StreamHandle {
                sender: task_sender,
                join,
            },
        );
        Ok(Some(sender))
    }

    pub async fn finish(&self, session_id: &str) {
        let handle = self.0.lock().remove(session_id);
        if let Some(handle) = handle {
            let _ = handle.sender.send(StreamMessage::Finish).await;
            let mut join = handle.join;
            if tokio::time::timeout(Duration::from_secs(20), &mut join)
                .await
                .is_err()
            {
                join.abort();
            }
        }
    }

    pub fn cancel(&self, session_id: &str) {
        if let Some(handle) = self.0.lock().remove(session_id) {
            let _ = handle.sender.try_send(StreamMessage::Cancel);
            handle.join.abort();
        }
    }
}

pub fn is_streaming_provider(provider: &str) -> bool {
    matches!(
        provider,
        "openai" | "xunfei" | "tencent" | "sherpa-streaming"
    )
}

async fn run_openai(
    app: AppHandle,
    session_id: &str,
    config: RecordingConfig,
    db: Database,
    mut audio: mpsc::Receiver<StreamMessage>,
) -> Result<(), AppError> {
    let key = secrets::get(&db, "asr-openai").or_else(|_| secrets::get(&db, "llm-openai"))?;
    let model = config
        .model
        .as_deref()
        .map(str::trim)
        .filter(|model| !model.is_empty())
        .unwrap_or("gpt-4o-mini-transcribe");
    let base = config
        .base_url
        .as_deref()
        .unwrap_or("https://api.openai.com/v1")
        .trim_end_matches('/');
    let ws_base = if let Some(rest) = base.strip_prefix("https://") {
        format!("wss://{rest}")
    } else if let Some(rest) = base.strip_prefix("http://") {
        format!("ws://{rest}")
    } else {
        base.to_owned()
    };
    let mut request = format!("{ws_base}/realtime?model={model}")
        .into_client_request()
        .map_err(|e| AppError::new("asr.realtime_url", e.to_string()))?;
    request.headers_mut().insert(
        "Authorization",
        format!("Bearer {key}")
            .parse()
            .map_err(|e| AppError::new("asr.auth", format!("{e}")))?,
    );
    let (socket, _) = connect_async(request).await.map_err(|e| {
        AppError::new("asr.network", format!("OpenAI Realtime 连接失败：{e}")).retryable()
    })?;
    let (mut writer, mut reader) = socket.split();
    let languages = config
        .language
        .as_deref()
        .filter(|value| !value.is_empty() && *value != "auto")
        .map(|value| vec![value]);
    writer
        .send(
            json!({
                "type": "session.update",
                "session": {
                    "type": "transcription",
                    "audio": { "input": {
                        "format": { "type": "audio/pcm", "rate": 24000 },
                        "transcription": {
                            "model": model,
                            "languages": languages,
                            "keywords": config.hotwords,
                            "delay": "low"
                        },
                        "turn_detection": null
                    }}
                }
            })
            .to_string()
            .into(),
        )
        .await
        .map_err(|e| AppError::new("asr.network", e.to_string()))?;

    let mut partial = String::new();
    let mut finishing = false;
    loop {
        tokio::select! {
            message = audio.recv(), if !finishing => match message {
                Some(StreamMessage::Audio(chunk)) => {
                    let pcm = pcm16(&resample_mono(&chunk, 24_000));
                    if !pcm.is_empty() {
                        writer.send(json!({"type":"input_audio_buffer.append","audio":BASE64.encode(pcm)}).to_string().into())
                            .await.map_err(|e| AppError::new("asr.network", e.to_string()))?;
                    }
                }
                Some(StreamMessage::Finish) | None => {
                    finishing = true;
                    writer.send(json!({"type":"input_audio_buffer.commit"}).to_string().into())
                        .await.map_err(|e| AppError::new("asr.network", e.to_string()))?;
                }
                Some(StreamMessage::Cancel) => break,
            },
            event = reader.next() => {
                let Some(event) = event else { break };
                let event = event.map_err(|e| AppError::new("asr.network", e.to_string()))?;
                if !event.is_text() { continue; }
                let value: Value = serde_json::from_str(event.to_text().unwrap_or_default())
                    .map_err(|e| AppError::new("asr.response", e.to_string()))?;
                match value["type"].as_str().unwrap_or_default() {
                    "conversation.item.input_audio_transcription.delta" => {
                        partial.push_str(value["delta"].as_str().unwrap_or_default());
                        crate::handle_stream_segment(&app, session_id, &partial, false)?;
                    }
                    "conversation.item.input_audio_transcription.completed" => {
                        let text = value["transcript"].as_str().unwrap_or(&partial);
                        crate::handle_stream_segment(&app, session_id, text, true)?;
                        break;
                    }
                    "error" => {
                        let message = value.pointer("/error/message").and_then(Value::as_str).unwrap_or("OpenAI Realtime 返回错误");
                        return Err(AppError::new("asr.realtime", message));
                    }
                    _ => {}
                }
            }
        }
    }
    let _ = writer.close().await;
    Ok(())
}

async fn run_xunfei(
    app: AppHandle,
    session_id: &str,
    db: Database,
    mut audio: mpsc::Receiver<StreamMessage>,
) -> Result<(), AppError> {
    let app_id = secrets::get(&db, "asr-xunfei-app-id")?;
    let api_key = secrets::get(&db, "asr-xunfei-api-key")?;
    let api_secret = secrets::get(&db, "asr-xunfei-api-secret")?;
    let host = "iat-api.xfyun.cn";
    let path = "/v2/iat";
    let date = httpdate::fmt_http_date(SystemTime::now());
    let signature_origin = format!("host: {host}\ndate: {date}\nGET {path} HTTP/1.1");
    let mut mac = Hmac::<Sha256>::new_from_slice(api_secret.as_bytes())
        .map_err(|e| AppError::new("asr.auth", e.to_string()))?;
    mac.update(signature_origin.as_bytes());
    let signature = BASE64.encode(mac.finalize().into_bytes());
    let authorization = BASE64.encode(format!(
        "api_key=\"{api_key}\", algorithm=\"hmac-sha256\", headers=\"host date request-line\", signature=\"{signature}\""
    ));
    let mut url = url::Url::parse(&format!("wss://{host}{path}"))
        .map_err(|e| AppError::new("asr.url", e.to_string()))?;
    url.query_pairs_mut()
        .append_pair("authorization", &authorization)
        .append_pair("date", &date)
        .append_pair("host", host);
    let (socket, _) = connect_async(url.as_str())
        .await
        .map_err(|e| AppError::new("asr.network", format!("讯飞流式连接失败：{e}")).retryable())?;
    let (mut writer, mut reader) = socket.split();
    let mut first = true;
    let mut finishing = false;
    let mut pieces = BTreeMap::<i64, String>::new();
    loop {
        tokio::select! {
            message = audio.recv(), if !finishing => match message {
                Some(StreamMessage::Audio(chunk)) => {
                    let pcm = pcm16(&resample_mono(&chunk, 16_000));
                    if pcm.is_empty() { continue; }
                    let status = if first { 0 } else { 1 };
                    let mut frame = json!({"data":{"status":status,"format":"audio/L16;rate=16000","encoding":"raw","audio":BASE64.encode(pcm)}});
                    if first {
                        frame["common"] = json!({"app_id":app_id});
                        frame["business"] = json!({"language":"zh_cn","domain":"iat","accent":"mandarin","dwa":"wpgs","vad_eos":3000});
                        first = false;
                    }
                    writer.send(frame.to_string().into()).await.map_err(|e| AppError::new("asr.network", e.to_string()))?;
                }
                Some(StreamMessage::Finish) | None => {
                    finishing = true;
                    writer.send(json!({"data":{"status":2,"format":"audio/L16;rate=16000","encoding":"raw","audio":""}}).to_string().into())
                        .await.map_err(|e| AppError::new("asr.network", e.to_string()))?;
                }
                Some(StreamMessage::Cancel) => break,
            },
            event = reader.next() => {
                let Some(event) = event else { break };
                let event = event.map_err(|e| AppError::new("asr.network", e.to_string()))?;
                if !event.is_text() { continue; }
                let value: Value = serde_json::from_str(event.to_text().unwrap_or_default())
                    .map_err(|e| AppError::new("asr.response", e.to_string()))?;
                if value["code"].as_i64().unwrap_or(0) != 0 {
                    return Err(AppError::new("asr.xunfei", value["message"].as_str().unwrap_or("讯飞识别失败")));
                }
                let text = update_xunfei_text(&value, &mut pieces);
                let stable = value.pointer("/data/status").and_then(Value::as_i64) == Some(2);
                if !text.is_empty() { crate::handle_stream_segment(&app, session_id, &text, stable)?; }
                if stable { break; }
            }
        }
    }
    let _ = writer.close().await;
    Ok(())
}

async fn run_tencent(
    app: AppHandle,
    session_id: &str,
    db: Database,
    mut audio: mpsc::Receiver<StreamMessage>,
) -> Result<(), AppError> {
    let url = tencent_url(&db)?;
    let (socket, _) = connect_async(url).await.map_err(|e| {
        AppError::new("asr.network", format!("腾讯云实时识别连接失败：{e}")).retryable()
    })?;
    let (mut writer, mut reader) = socket.split();
    let mut finishing = false;
    loop {
        tokio::select! {
            message = audio.recv(), if !finishing => match message {
                Some(StreamMessage::Audio(chunk)) => {
                    let pcm = pcm16(&resample_mono(&chunk, 16_000));
                    if !pcm.is_empty() {
                        writer.send(tokio_tungstenite::tungstenite::Message::Binary(pcm.into())).await
                            .map_err(|e| AppError::new("asr.network", e.to_string()))?;
                    }
                }
                Some(StreamMessage::Finish) | None => {
                    finishing = true;
                    writer.send(tencent_end_message()).await
                        .map_err(|e| AppError::new("asr.network", e.to_string()))?;
                }
                Some(StreamMessage::Cancel) => break,
            },
            event = reader.next() => {
                let Some(event) = event else { break };
                let event = event.map_err(|e| AppError::new("asr.network", e.to_string()))?;
                if !event.is_text() { continue; }
                let value: Value = serde_json::from_str(event.to_text().unwrap_or_default())
                    .map_err(|e| AppError::new("asr.response", e.to_string()))?;
                if value["code"].as_i64().unwrap_or(0) != 0 {
                    return Err(AppError::new("asr.tencent", value["message"].as_str().unwrap_or("腾讯云识别失败")));
                }
                let (text, stable, final_event) = tencent_result(&value);
                if !text.is_empty() {
                    crate::handle_stream_segment(&app, session_id, text, stable)?;
                }
                if final_event { break; }
            }
        }
    }
    let _ = writer.close().await;
    Ok(())
}

fn tencent_end_message() -> tokio_tungstenite::tungstenite::Message {
    json!({"type":"end"}).to_string().into()
}

fn tencent_result(value: &Value) -> (&str, bool, bool) {
    let text = value
        .pointer("/result/voice_text_str")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let stable = value.pointer("/result/slice_type").and_then(Value::as_i64) == Some(2);
    let final_event = value["final"].as_i64() == Some(1);
    (text, stable, final_event)
}

pub async fn test_tencent_connection(db: &Database) -> Result<(), AppError> {
    let url = tencent_url(db)?;
    let (mut socket, _) = connect_async(url).await.map_err(|e| {
        AppError::new("asr.network", format!("腾讯云实时识别连接失败：{e}")).retryable()
    })?;
    let event = tokio::time::timeout(Duration::from_secs(10), socket.next())
        .await
        .map_err(|_| AppError::new("asr.network", "腾讯云连接测试超时").retryable())?
        .ok_or_else(|| AppError::new("asr.network", "腾讯云在握手后关闭了连接"))?
        .map_err(|e| AppError::new("asr.network", e.to_string()).retryable())?;
    if !event.is_text() {
        return Err(AppError::new("asr.response", "腾讯云握手返回了未知消息"));
    }
    let value: Value = serde_json::from_str(event.to_text().unwrap_or_default())
        .map_err(|e| AppError::new("asr.response", e.to_string()))?;
    if value["code"].as_i64().unwrap_or(-1) != 0 {
        return Err(AppError::new(
            "asr.tencent",
            value["message"].as_str().unwrap_or("腾讯云连接测试失败"),
        ));
    }
    let _ = socket.close(None).await;
    Ok(())
}

fn tencent_url(db: &Database) -> Result<String, AppError> {
    let app_id = secrets::get(db, "asr-tencent-app-id")?;
    let secret_id = secrets::get(db, "asr-tencent-secret-id")?;
    let secret_key = secrets::get(db, "asr-tencent-secret-key")?;
    let host = "asr.cloud.tencent.com";
    let path = format!("/asr/v2/{app_id}");
    let now = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map_err(|e| AppError::new("asr.auth", e.to_string()))?
        .as_secs();
    let mut params = BTreeMap::from([
        ("engine_model_type", "16k_zh".to_owned()),
        ("expired", (now + 86_400).to_string()),
        ("nonce", now.to_string()),
        ("secretid", secret_id),
        ("timestamp", now.to_string()),
        ("voice_format", "1".to_owned()),
        ("voice_id", uuid::Uuid::new_v4().to_string()),
    ]);
    let sign_text = format!("{host}{path}?{}", form_query(&params));
    let mut mac = Hmac::<Sha1>::new_from_slice(secret_key.as_bytes())
        .map_err(|e| AppError::new("asr.auth", e.to_string()))?;
    mac.update(sign_text.as_bytes());
    params.insert("signature", BASE64.encode(mac.finalize().into_bytes()));
    Ok(format!("wss://{host}{path}?{}", form_query(&params)))
}

fn form_query(params: &BTreeMap<&str, String>) -> String {
    let mut serializer = url::form_urlencoded::Serializer::new(String::new());
    for (key, value) in params {
        serializer.append_pair(key, value);
    }
    serializer.finish()
}

async fn run_local(
    app: AppHandle,
    session_id: &str,
    config: RecordingConfig,
    app_data: PathBuf,
    audio: mpsc::Receiver<StreamMessage>,
) -> Result<(), AppError> {
    let model = config
        .model
        .as_deref()
        .filter(|value| !value.is_empty())
        .map(std::path::PathBuf::from)
        .ok_or_else(|| AppError::new("asr.model_missing", "请先下载 Sherpa 流式模型"))?;
    let device = config.compute_device.clone();
    let app_for_worker = app.clone();
    let sid = session_id.to_owned();
    tokio::task::spawn_blocking(move || {
        crate::local_asr::stream(&app_data, &model, &device, audio, |text, stable| {
            crate::handle_stream_segment(&app_for_worker, &sid, text, stable)
        })
    })
    .await
    .map_err(|e| AppError::new("asr.runtime", e.to_string()))?
}

fn update_xunfei_text(value: &Value, pieces: &mut BTreeMap<i64, String>) -> String {
    let Some(result) = value.pointer("/data/result") else {
        return String::new();
    };
    if result["pgs"].as_str() == Some("rpl") {
        if let Some(range) = result["rg"].as_array() {
            if range.len() == 2 {
                let start = range[0].as_i64().unwrap_or(0);
                let end = range[1].as_i64().unwrap_or(start);
                pieces.retain(|key, _| *key < start || *key > end);
            }
        }
    }
    let text = result["ws"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|word| word["cw"].as_array()?.first()?.get("w")?.as_str())
        .collect::<String>();
    if let Some(sn) = result["sn"].as_i64() {
        pieces.insert(sn, text);
    }
    pieces.values().cloned().collect()
}

fn resample_mono(chunk: &AudioChunk, target_rate: u32) -> Vec<f32> {
    let channels = chunk.channels.max(1) as usize;
    let mono: Vec<f32> = chunk
        .samples
        .chunks(channels)
        .map(|frame| frame.iter().sum::<f32>() / frame.len().max(1) as f32)
        .collect();
    if chunk.sample_rate == target_rate {
        return mono;
    }
    if mono.is_empty() {
        return mono;
    }
    let ratio = chunk.sample_rate as f64 / target_rate as f64;
    let output_len = (mono.len() as f64 / ratio).floor() as usize;
    (0..output_len)
        .map(|index| {
            let position = index as f64 * ratio;
            let left = position.floor() as usize;
            let right = (left + 1).min(mono.len() - 1);
            let fraction = (position - left as f64) as f32;
            mono[left] * (1.0 - fraction) + mono[right] * fraction
        })
        .collect()
}

fn pcm16(samples: &[f32]) -> Vec<u8> {
    samples
        .iter()
        .flat_map(|sample| ((sample.clamp(-1.0, 1.0) * i16::MAX as f32) as i16).to_le_bytes())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn downmix_and_resample_produces_expected_rate() {
        let chunk = AudioChunk {
            samples: (0..480).flat_map(|_| [1.0, -1.0]).collect(),
            sample_rate: 48_000,
            channels: 2,
        };
        assert_eq!(resample_mono(&chunk, 16_000).len(), 160);
    }

    #[test]
    fn xunfei_dynamic_replacement_rewrites_prior_piece() {
        let mut pieces = BTreeMap::new();
        let first = json!({"data":{"result":{"sn":1,"ws":[{"cw":[{"w":"语留"}]}]}}});
        let replacement =
            json!({"data":{"result":{"sn":2,"pgs":"rpl","rg":[1,1],"ws":[{"cw":[{"w":"语流"}]}]}}});
        assert_eq!(update_xunfei_text(&first, &mut pieces), "语留");
        assert_eq!(update_xunfei_text(&replacement, &mut pieces), "语流");
    }

    #[test]
    fn tencent_result_reads_nested_partial_and_stable_segments() {
        let partial = json!({"code":0,"result":{"slice_type":1,"voice_text_str":"腾讯"}});
        let stable = json!({"code":0,"result":{"slice_type":2,"voice_text_str":"腾讯云"}});
        assert_eq!(tencent_result(&partial), ("腾讯", false, false));
        assert_eq!(tencent_result(&stable), ("腾讯云", true, false));
    }

    #[test]
    fn tencent_stream_only_finishes_on_the_final_event() {
        let sentence = json!({"code":0,"result":{"slice_type":2,"voice_text_str":"第一句"}});
        let final_event = json!({"code":0,"final":1});
        assert_eq!(tencent_result(&sentence), ("第一句", true, false));
        assert_eq!(tencent_result(&final_event), ("", false, true));
        assert_eq!(
            tencent_end_message().to_text().unwrap(),
            r#"{"type":"end"}"#
        );
    }
}
