mod asr;
mod audio;
mod data_directory;
mod db;
mod diagnostics;
mod error;
mod hotkeys;
mod injection;
mod llm;
mod local_asr;
mod local_models;
mod models;
mod secrets;
mod streaming_asr;

use audio::AudioManager;
use db::Database;
use diagnostics::DiagnosticLog;
use error::AppError;
use injection::InjectionManager;
use hotkeys::{
    arm_hold_release_pending, bind_hold_release_pending, classify_shortcut, clear_hold_release_pending,
    ensure_hotkey_defaults, hold_release_action, hotkey_config, registered_user_shortcuts,
    session_wake_behavior, take_hold_release_pending, HoldReleaseAction, HoldReleasePending,
    HotkeyAction, WakeMode, WakeSessionBehavior,
};
use models::{
    AudioDevice, FocusTarget, Hotword, InjectionState, LlmRequest, LocalModelInfo, PromptTemplate,
    ProviderTest, RecordItem, RecordingConfig, RecordingReview, RecordingSession,
    TranscriptSegment,
};
use parking_lot::Mutex;
use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, OnceLock,
    },
    time::{Duration, Instant},
};
use tauri_plugin_autostart::ManagerExt;
use tauri::{
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
};
use tauri::{AppHandle, Emitter, Manager, WebviewUrl, WebviewWindowBuilder};
use tauri::webview::PageLoadEvent;
use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};
use uuid::Uuid;

pub struct AppState {
    db: Database,
    data_root: PathBuf,
    default_data_root: PathBuf,
    data_location_file: PathBuf,
    data_migration_pending: AtomicBool,
    audio: AudioManager,
    injection: InjectionManager,
    sessions: Mutex<HashMap<String, RecordingSession>>,
    active_session: Arc<Mutex<Option<String>>>,
    model_downloads: local_models::DownloadFlags,
    session_configs: Mutex<HashMap<String, RecordingConfig>>,
    session_hotwords: Mutex<HashMap<String, Vec<String>>>,
    session_raw_text: Mutex<HashMap<String, String>>,
    settings: Mutex<serde_json::Value>,
    hotwords: Mutex<Vec<Hotword>>,
    streaming: streaming_asr::StreamingManager,
    diagnostics: DiagnosticLog,
    wake_pressed_at: Mutex<Option<Instant>>,
    hold_release_pending: Mutex<Option<HoldReleasePending>>,
}

const AUTOSTART_ARG: &str = "--from-autostart";
const FINAL_TRANSCRIPTION_TIMEOUT: Duration = Duration::from_secs(60);
static REVIEW_SHORTCUT_DESIRED: AtomicBool = AtomicBool::new(false);
static REVIEW_SHORTCUT_GATE: OnceLock<std::sync::Mutex<()>> = OnceLock::new();
static ESCAPE_SHORTCUT_DESIRED: AtomicBool = AtomicBool::new(false);
static ESCAPE_SHORTCUT_GATE: OnceLock<std::sync::Mutex<()>> = OnceLock::new();

fn ensure_data_writable(state: &AppState) -> Result<(), AppError> {
    if state.data_migration_pending.load(Ordering::Relaxed) {
        Err(AppError::new(
            "data.migration_busy",
            "数据目录正在迁移或正在等待重启",
        ))
    } else {
        Ok(())
    }
}

fn streamed_session_text(committed: Option<&str>, raw: Option<&str>) -> Option<String> {
    committed
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .map(str::to_owned)
        .or_else(|| {
            raw.map(str::trim)
                .filter(|text| !text.is_empty())
                .map(str::to_owned)
        })
}

fn emit_voice_status(
    app: &AppHandle,
    session_id: Option<&str>,
    phase: &str,
    message: Option<&str>,
) {
    let _ = app.emit(
        "voice-process-status",
        serde_json::json!({
            "sessionId": session_id,
            "phase": phase,
            "message": message,
        }),
    );
}

fn diagnostic(
    state: &AppState,
    level: &str,
    event: &str,
    session_id: Option<&str>,
    details: serde_json::Value,
) {
    state.diagnostics.write(level, event, session_id, details);
}

fn emit_transcript_event(
    app: &AppHandle,
    event: &str,
    session_id: &str,
    segment: &TranscriptSegment,
) {
    let _ = app.emit(
        event,
        serde_json::json!({
            "sessionId": session_id,
            "text": segment.text,
            "stable": segment.stable,
            "startMs": segment.start_ms,
            "endMs": segment.end_ms,
            "confidence": segment.confidence,
        }),
    );
}

fn set_review_shortcuts(app: &AppHandle, enabled: bool) {
    let shortcuts = app.global_shortcut();
    if enabled {
        let _ = shortcuts.register("Enter");
        let _ = shortcuts.register("Ctrl+Enter");
    } else {
        let _ = shortcuts.unregister("Enter");
        let _ = shortcuts.unregister("Ctrl+Enter");
    }
}

fn set_review_shortcuts_background(app: &AppHandle, enabled: bool) {
    REVIEW_SHORTCUT_DESIRED.store(enabled, Ordering::Release);
    let shortcut_app = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let Ok(_guard) = REVIEW_SHORTCUT_GATE
            .get_or_init(|| std::sync::Mutex::new(()))
            .lock()
        else {
            return;
        };
        set_review_shortcuts(
            &shortcut_app,
            REVIEW_SHORTCUT_DESIRED.load(Ordering::Acquire),
        );
    });
}

fn set_escape_shortcut(app: &AppHandle, enabled: bool) {
    let shortcuts = app.global_shortcut();
    if enabled {
        let _ = shortcuts.register("Escape");
    } else {
        let _ = shortcuts.unregister("Escape");
    }
}

fn set_escape_shortcut_background(app: &AppHandle, enabled: bool) {
    ESCAPE_SHORTCUT_DESIRED.store(enabled, Ordering::Release);
    let shortcut_app = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let Ok(_guard) = ESCAPE_SHORTCUT_GATE
            .get_or_init(|| std::sync::Mutex::new(()))
            .lock()
        else {
            return;
        };
        set_escape_shortcut(
            &shortcut_app,
            ESCAPE_SHORTCUT_DESIRED.load(Ordering::Acquire),
        );
    });
}

fn tray_tooltip(wake: &str) -> String {
    format!("语流 · {wake} 开始录音")
}

fn update_tray_tooltip(app: &AppHandle, wake: &str) {
    if let Some(tray) = app.tray_by_id("main") {
        let _ = tray.set_tooltip(Some(tray_tooltip(wake)));
    }
}

fn register_user_shortcuts(app: &AppHandle, config: &hotkeys::HotkeyConfig) -> Result<(), AppError> {
    register_user_shortcuts_with_policy(app, config, true)
}

fn register_user_shortcuts_with_policy(
    app: &AppHandle,
    config: &hotkeys::HotkeyConfig,
    strict: bool,
) -> Result<(), AppError> {
    let shortcuts = app.global_shortcut();
    for accelerator in registered_user_shortcuts(config) {
        if accelerator.is_empty() {
            continue;
        }
        if let Err(error) = shortcuts.register(accelerator.as_str()) {
            if strict {
                return Err(AppError::new(
                    "hotkey.register",
                    format!("无法注册快捷键 {accelerator}：{error}"),
                ));
            }
            diagnostic(
                &app.state::<AppState>(),
                "warn",
                "hotkey.register_failed",
                None,
                serde_json::json!({
                    "accelerator": accelerator,
                    "error": error.to_string(),
                }),
            );
        }
    }
    Ok(())
}

fn reregister_user_shortcuts(
    app: &AppHandle,
    previous: &hotkeys::HotkeyConfig,
    next: &hotkeys::HotkeyConfig,
) -> Result<(), AppError> {
    let shortcuts = app.global_shortcut();
    for accelerator in registered_user_shortcuts(previous) {
        let _ = shortcuts.unregister(accelerator.as_str());
    }
    register_user_shortcuts(app, next)?;
    update_tray_tooltip(app, &next.wake);
    Ok(())
}

fn apply_autostart(app: &AppHandle, enabled: bool) -> Result<(), AppError> {
    let manager = app.autolaunch();
    if enabled {
        manager
            .enable()
            .map_err(|error| AppError::new("autostart.enable", error.to_string()))
    } else {
        manager
            .disable()
            .map_err(|error| AppError::new("autostart.disable", error.to_string()))
    }
}

fn apply_retention(state: &AppState) {
    let days = state
        .settings
        .lock()
        .get("retentionDays")
        .and_then(|value| value.as_i64())
        .unwrap_or(30);
    let _ = state.db.delete_expired_records(days);
}

fn toggle_hotwords_enabled(app: &AppHandle, state: &AppState) {
    if ensure_data_writable(state).is_err() {
        return;
    }
    let mut settings = state.settings.lock();
    let enabled = settings
        .get("hotwordsEnabled")
        .and_then(|value| value.as_bool())
        .unwrap_or(true);
    if let Some(object) = settings.as_object_mut() {
        object.insert("hotwordsEnabled".into(), serde_json::Value::Bool(!enabled));
    }
    let snapshot = settings.clone();
    drop(settings);
    let _ = state.db.set_setting("appSettings", &snapshot.to_string());
    let _ = app.emit(
        "settings-changed",
        serde_json::json!({"key": "appSettings", "value": snapshot}),
    );
}

fn start_recording_from_shortcut(app: &AppHandle) {
    let state = app.state::<AppState>();
    let config = saved_recording_config(&state.settings.lock());
    let target = injection::capture_foreground();
    let task_app = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let state = task_app.state::<AppState>();
        if let Err(error) = begin_recording(&task_app, &state, target, config) {
            if error.code != "session.active" {
                let _ = task_app.emit("session-error", error);
            }
        }
    });
}

fn apply_hold_release(app: &AppHandle, session_id: &str, action: HoldReleaseAction) {
    if action == HoldReleaseAction::Cancel {
        let task_app = app.clone();
        let cancel_id = session_id.to_owned();
        tauri::async_runtime::spawn(async move {
            let state = task_app.state::<AppState>();
            let _ = cancel_recording_impl(&task_app, &state, &cancel_id).await;
        });
    } else {
        emit_session_shortcut(app, false, session_id);
    }
}

fn consume_hold_release_after_listening(app: &AppHandle, state: &AppState, session_id: &str) {
    let action = {
        let mut pending = state.hold_release_pending.lock();
        bind_hold_release_pending(&mut pending, session_id);
        take_hold_release_pending(&mut pending, session_id)
    };
    let Some(action) = action else {
        return;
    };
    apply_hold_release(app, session_id, action);
}

fn emit_session_shortcut(app: &AppHandle, reviewing: bool, session_id: &str) {
    let _ = app.emit(
        if reviewing {
            "shortcut-cancel-requested"
        } else {
            "shortcut-stop-requested"
        },
        serde_json::json!({"sessionId": session_id}),
    );
}

fn handle_classified_shortcut(app: &AppHandle, action: HotkeyAction, pressed: bool) {
    let state = app.state::<AppState>();
    let config = hotkey_config(&state.settings.lock());
    match action {
        HotkeyAction::Escape => {
            if !pressed {
                return;
            }
            if let Some(session_id) = state.active_session.lock().clone() {
                let task_app = app.clone();
                tauri::async_runtime::spawn(async move {
                    let state = task_app.state::<AppState>();
                    let _ = cancel_recording_impl(&task_app, &state, &session_id).await;
                });
            }
        }
        HotkeyAction::ReviewEnter | HotkeyAction::ReviewAiEnter => {
            if !pressed {
                return;
            }
            if let Some(session_id) = state.active_session.lock().clone() {
                let reviewing = state
                    .sessions
                    .lock()
                    .get(&session_id)
                    .is_some_and(|session| session.state == "reviewing");
                if reviewing {
                    let _ = app.emit(
                        "shortcut-review-accept",
                        serde_json::json!({
                            "sessionId": session_id,
                            "ai": action == HotkeyAction::ReviewAiEnter
                        }),
                    );
                }
            }
        }
        HotkeyAction::Panel => {
            if pressed {
                toggle_main(app);
            }
        }
        HotkeyAction::Mute => {
            if pressed {
                toggle_hotwords_enabled(app, &state);
            }
        }
        HotkeyAction::Ai => {
            if !pressed {
                return;
            }
            if let Some(session_id) = state.active_session.lock().clone() {
                let reviewing = state
                    .sessions
                    .lock()
                    .get(&session_id)
                    .is_some_and(|session| session.state == "reviewing");
                if reviewing {
                    let _ = app.emit(
                        "shortcut-review-accept",
                        serde_json::json!({
                            "sessionId": session_id,
                            "ai": true
                        }),
                    );
                }
            }
        }
        HotkeyAction::Wake => {
            let active = state
                .active_session
                .lock()
                .clone()
                .filter(|session_id| state.sessions.lock().contains_key(session_id));
            if config.mode == WakeMode::Hold {
                if pressed {
                    if active.is_none() {
                        clear_hold_release_pending(
                            &mut state.hold_release_pending.lock(),
                            None,
                        );
                        *state.wake_pressed_at.lock() = Some(Instant::now());
                        start_recording_from_shortcut(app);
                    }
                    return;
                }
                let pressed_at = state.wake_pressed_at.lock().take();
                let action = pressed_at
                    .map(|instant| hold_release_action(instant, config.hold_ms, Instant::now()))
                    .unwrap_or(HoldReleaseAction::Finish);
                let Some(session_id) = active else {
                    arm_hold_release_pending(
                        &mut state.hold_release_pending.lock(),
                        action,
                    );
                    return;
                };
                match session_wake_behavior(
                    &state
                        .sessions
                        .lock()
                        .get(&session_id)
                        .map(|session| session.state.as_str())
                        .unwrap_or_default(),
                ) {
                    WakeSessionBehavior::ToggleCancelHoldIgnore
                    | WakeSessionBehavior::IgnoreWake => return,
                    WakeSessionBehavior::StopOrFinish => {
                        apply_hold_release(app, &session_id, action);
                    }
                }
                return;
            }
            if !pressed {
                return;
            }
            if let Some(session_id) = active {
                match session_wake_behavior(
                    &state
                        .sessions
                        .lock()
                        .get(&session_id)
                        .map(|session| session.state.as_str())
                        .unwrap_or_default(),
                ) {
                    WakeSessionBehavior::IgnoreWake => {}
                    WakeSessionBehavior::ToggleCancelHoldIgnore => {
                        emit_session_shortcut(app, true, &session_id);
                    }
                    WakeSessionBehavior::StopOrFinish => {
                        emit_session_shortcut(app, false, &session_id);
                    }
                }
            } else {
                start_recording_from_shortcut(app);
            }
        }
    }
}

fn reserve_active_session(
    active_session: &Mutex<Option<String>>,
    session_id: &str,
) -> Result<(), AppError> {
    let mut active = active_session.lock();
    if active.is_some() {
        return Err(AppError::new("session.active", "已有录音正在进行"));
    }
    *active = Some(session_id.to_owned());
    Ok(())
}

fn clear_active_session(active_session: &Mutex<Option<String>>, session_id: &str) -> bool {
    let mut active = active_session.lock();
    if active.as_deref() != Some(session_id) {
        return false;
    }
    *active = None;
    true
}

async fn run_blocking<T, F>(operation: F) -> Result<T, AppError>
where
    T: Send + 'static,
    F: FnOnce() -> Result<T, AppError> + Send + 'static,
{
    tauri::async_runtime::spawn_blocking(operation)
        .await
        .map_err(|error| AppError::new("runtime.join", error.to_string()))?
}

fn migrate_app_settings(mut settings: serde_json::Value) -> (serde_json::Value, bool) {
    if !settings.is_object() {
        settings = serde_json::json!({});
    }
    let legacy_before_solution_a = settings.get("autoEndOnSilence").is_none();
    let needs_direct_insert = settings.get("directInsert").is_none();
    let needs_auto_format = settings.get("autoFormat").is_none();
    let direct_insert = settings
        .get("liveInsert")
        .and_then(|value| value.as_bool())
        .unwrap_or(true);
    let llm_provider = settings
        .get("llmProvider")
        .and_then(|value| value.as_str())
        .unwrap_or("openai");
    let auto_format = settings
        .get("llm")
        .and_then(|value| value.get(llm_provider))
        .and_then(|value| value.get("autoFormat"))
        .and_then(|value| value.as_bool())
        .unwrap_or(false);
    let mut changed = legacy_before_solution_a || needs_direct_insert || needs_auto_format;
    changed |= ensure_hotkey_defaults(&mut settings);
    changed |= migrate_openai_realtime_model(&mut settings);
    if let Some(object) = settings.as_object_mut() {
        if legacy_before_solution_a {
            object.insert("autoEndOnSilence".into(), serde_json::Value::Bool(true));
            object.insert("bubbleStyle".into(), serde_json::Value::String("A".into()));
        }
        if needs_direct_insert {
            object.insert(
                "directInsert".into(),
                serde_json::Value::Bool(direct_insert),
            );
        }
        if needs_auto_format {
            object.insert("autoFormat".into(), serde_json::Value::Bool(auto_format));
        }
        if object.get("hideOnStart").is_none() {
            object.insert("hideOnStart".into(), serde_json::Value::Bool(true));
            changed = true;
        }
        changed |= object.remove("hideOnStartUserSet").is_some();
        changed |= object.remove("liveInsert").is_some();
        if let Some(providers) = object
            .get_mut("llm")
            .and_then(|value| value.as_object_mut())
        {
            for config in providers.values_mut() {
                if let Some(config) = config.as_object_mut() {
                    changed |= config.remove("autoFormat").is_some();
                    if config.get("thinking").is_none() {
                        config.insert("thinking".into(), serde_json::Value::Bool(false));
                        changed = true;
                    }
                }
            }
        }
    }
    (settings, changed)
}

fn migrate_openai_realtime_model(settings: &mut serde_json::Value) -> bool {
    const LEGACY: &[&str] = &["whisper-1", "gpt-live-transcribe"];
    const DEFAULT_MODEL: &str = "gpt-4o-mini-transcribe";
    let Some(object) = settings.as_object_mut() else {
        return false;
    };
    let provider = object
        .get("asrProvider")
        .and_then(|value| value.as_str())
        .unwrap_or("openai")
        .trim();
    if !provider.eq_ignore_ascii_case("openai") {
        return false;
    }
    let model = object
        .get("asrModel")
        .and_then(|value| value.as_str())
        .unwrap_or("")
        .trim();
    if !LEGACY.iter().any(|legacy| model.eq_ignore_ascii_case(legacy)) && !model.is_empty() {
        return false;
    }
    object.insert(
        "asrModel".into(),
        serde_json::Value::String(DEFAULT_MODEL.into()),
    );
    true
}

fn begin_recording(
    app: &AppHandle,
    state: &AppState,
    target: FocusTarget,
    mut config: RecordingConfig,
) -> Result<RecordingSession, AppError> {
    if let Err(error) = ensure_data_writable(state) {
        clear_hold_release_pending(&mut state.hold_release_pending.lock(), None);
        return Err(error);
    }
    let id = Uuid::new_v4().to_string();
    if let Err(error) = reserve_active_session(&state.active_session, &id) {
        clear_hold_release_pending(&mut state.hold_release_pending.lock(), None);
        return Err(error);
    }
    let hotwords_enabled = state
        .settings
        .lock()
        .get("hotwordsEnabled")
        .and_then(|value| value.as_bool())
        .unwrap_or(true);
    config.hotwords = if hotwords_enabled {
        state
            .hotwords
            .lock()
            .iter()
            .filter(|item| item.enabled)
            .map(|item| item.word.clone())
            .collect()
    } else {
        Vec::new()
    };
    diagnostic(
        state,
        "info",
        "recording.begin",
        Some(&id),
        serde_json::json!({
            "provider": config.provider,
            "model": config.model,
            "deviceConfigured": config.device_id.is_some(),
            "deviceId": config.device_id,
        }),
    );
    // The bubble is created after the main window is shown, then kept hidden.
    // Showing it here would expose the stale idle-sized viewport.
    let _ = ensure_bubble_window(app);
    emit_voice_status(app, Some(&id), "waking", None);
    let session = RecordingSession {
        id: id.clone(),
        state: "capturing".into(),
        target: target.clone(),
        started_at: chrono::Utc::now().to_rfc3339(),
        committed_text: String::new(),
    };
    state.injection.begin(&id, target);
    state.sessions.lock().insert(id.clone(), session.clone());
    state
        .session_configs
        .lock()
        .insert(id.clone(), config.clone());
    state.session_hotwords.lock().insert(id.clone(), Vec::new());
    state
        .session_raw_text
        .lock()
        .insert(id.clone(), String::new());
    let stream_sender = match state.streaming.start(
        app.clone(),
        id.clone(),
        config.clone(),
        state.db.clone(),
        state.data_root.clone(),
    ) {
        Ok(sender) => sender,
        Err(error) => {
            diagnostic(
                state,
                "error",
                "recording.stream_start_failed",
                Some(&id),
                serde_json::json!({
                    "code": error.code,
                    "message": error.message,
                }),
            );
            clear_hold_release_pending(&mut state.hold_release_pending.lock(), Some(&id));
            cleanup_session(state, &id);
            emit_voice_status(app, Some(&id), "failed", Some(&error.message));
            return Err(error);
        }
    };
    if let Err(error) = audio::start(
        app.clone(),
        &state.audio,
        &id,
        config.device_id.as_deref(),
        stream_sender,
    ) {
        diagnostic(
            state,
            "error",
            "recording.audio_start_failed",
            Some(&id),
            serde_json::json!({
                "code": error.code,
                "message": error.message,
            }),
        );
        state.streaming.cancel(&id);
        clear_hold_release_pending(&mut state.hold_release_pending.lock(), Some(&id));
        cleanup_session(state, &id);
        emit_voice_status(app, Some(&id), "failed", Some(&error.message));
        return Err(error);
    }
    let _ = app.emit("recording-state", &session);
    emit_voice_status(app, Some(&id), "listening", None);
    set_escape_shortcut_background(app, true);
    consume_hold_release_after_listening(app, state, &id);
    diagnostic(
        state,
        "info",
        "recording.listening",
        Some(&id),
        serde_json::json!({}),
    );
    warmup_local_recognizer(&config, state.data_root.clone());
    Ok(session)
}

fn warmup_local_recognizer(config: &RecordingConfig, data_root: PathBuf) {
    let provider = config.provider.clone();
    let Some(model) = config
        .model
        .as_deref()
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
    else {
        return;
    };
    let language = config.language.clone();
    let compute_device = config.compute_device.clone();
    if provider != "qwen3-asr" && provider != "faster-whisper" {
        return;
    }
    tauri::async_runtime::spawn_blocking(move || {
        let _ = crate::asr::warmup_local(
            &data_root,
            &model,
            &provider,
            language.as_deref(),
            &compute_device,
        );
    });
}

#[tauri::command]
async fn start_recording(
    app: AppHandle,
    target: Option<FocusTarget>,
) -> Result<RecordingSession, AppError> {
    let task_app = app.clone();
    run_blocking(move || {
        let state = task_app.state::<AppState>();
        let config = saved_recording_config(&state.settings.lock());
        begin_recording(
            &task_app,
            &state,
            target.unwrap_or_else(injection::capture_foreground),
            config,
        )
    })
    .await
}

#[tauri::command]
async fn stop_recording(
    app: AppHandle,
    state: tauri::State<'_, AppState>,
    session_id: String,
) -> Result<(), AppError> {
    audio::stop_if_active(&state.audio, &session_id);
    if state.streaming.contains(&session_id) {
        state.streaming.finish(&session_id).await;
    }
    if let Some(session) = state.sessions.lock().get_mut(&session_id) {
        session.state = "finalizing".into();
    }
    finalize_session(&app, &state, &session_id, None, false, None, None)?;
    emit_voice_status(
        &app,
        Some(&session_id),
        "completed",
        Some("识别和写入均已完成"),
    );
    Ok(())
}

fn session_duration(started_at: &str) -> u64 {
    chrono::DateTime::parse_from_rfc3339(started_at)
        .map(|started| {
            (chrono::Utc::now() - started.with_timezone(&chrono::Utc))
                .num_seconds()
                .max(0) as u64
        })
        .unwrap_or(0)
}

fn review_should_finish_stream(has_stream: bool) -> bool {
    has_stream
}

fn finalize_session(
    app: &AppHandle,
    state: &AppState,
    session_id: &str,
    final_text: Option<String>,
    ai: bool,
    llm_provider: Option<String>,
    llm_model: Option<String>,
) -> Result<(), AppError> {
    state.injection.finish(session_id);
    let mut session = state.sessions.lock().remove(session_id);
    let hotwords = state
        .session_hotwords
        .lock()
        .remove(session_id)
        .unwrap_or_default();
    state.session_configs.lock().remove(session_id);
    let captured_raw = state.session_raw_text.lock().remove(session_id);
    clear_active_session(&state.active_session, session_id);
    set_review_shortcuts_background(app, false);
    set_escape_shortcut_background(app, false);
    let mut persistence_error = None;
    if let Some(session) = session.as_mut() {
        let raw_text = captured_raw
            .filter(|text| !text.trim().is_empty())
            .unwrap_or_else(|| session.committed_text.clone());
        if let Some(text) = final_text {
            session.committed_text = text;
        }
        let record = RecordItem {
            id: session.id.clone(),
            started_at: session.started_at.clone(),
            app: session.target.title.clone(),
            duration: session_duration(&session.started_at),
            chars: raw_text.chars().count(),
            hotwords,
            ai,
            llm_provider,
            model: llm_model,
            text: raw_text,
            formatted_text: ai.then(|| session.committed_text.clone()),
        };
        match state.db.insert_record(&record) {
            Ok(()) => {
                let _ = app.emit("record-created", &record);
            }
            Err(error) => persistence_error = Some(error),
        }
    }
    if let Some(error) = persistence_error {
        let _ = app.emit(
            "session-warning",
            serde_json::json!({
                "sessionId": session_id,
                "code": "record.persist",
                "message": format!("文字已成功处理，但历史记录保存失败：{}", error.message),
            }),
        );
    }
    let _ = app.emit(
        "recording-stopped",
        serde_json::json!({"sessionId":session_id}),
    );
    Ok(())
}

#[tauri::command]
async fn stop_recording_for_review(
    app: AppHandle,
    state: tauri::State<'_, AppState>,
    session_id: String,
    transcript: Option<String>,
    show_review: bool,
) -> Result<RecordingReview, AppError> {
    let started = std::time::Instant::now();
    audio::stop_if_active(&state.audio, &session_id);
    let has_stream = state.streaming.contains(&session_id);
    diagnostic(
        &state,
        "info",
        "review.prepare_started",
        Some(&session_id),
        serde_json::json!({
            "hasStream": has_stream,
            "waitForStream": review_should_finish_stream(has_stream),
            "providedTranscriptLength": transcript.as_deref().map(str::chars).map(Iterator::count).unwrap_or(0),
        }),
    );
    if review_should_finish_stream(has_stream) {
        let stream_finish_started = std::time::Instant::now();
        state.streaming.finish(&session_id).await;
        diagnostic(
            &state,
            "info",
            "review.stream_finish_completed",
            Some(&session_id),
            serde_json::json!({
                "elapsedMs": stream_finish_started.elapsed().as_millis(),
            }),
        );
    }
    let (text, started_at) = {
        let mut sessions = state.sessions.lock();
        let session = sessions
            .get_mut(&session_id)
            .ok_or_else(|| AppError::new("session.missing", "录音会话不存在"))?;
        if let Some(text) = transcript.filter(|text| !text.trim().is_empty()) {
            session.committed_text = text;
        } else if session.committed_text.trim().is_empty() {
            if let Some(raw) = state
                .session_raw_text
                .lock()
                .get(&session_id)
                .cloned()
                .filter(|text| !text.trim().is_empty())
            {
                session.committed_text = raw;
            }
        }
        session.state = if show_review {
            "reviewing"
        } else {
            "inserting"
        }
        .into();
        (session.committed_text.clone(), session.started_at.clone())
    };
    // Read both values under one guard. Two adjacent lock expressions can keep
    // the first temporary guard alive through the whole struct expression.
    let review_config = state.session_configs.lock().get(&session_id).cloned();
    let review = RecordingReview {
        session_id: session_id.clone(),
        text,
        duration: session_duration(&started_at),
        hotwords: state
            .session_hotwords
            .lock()
            .get(&session_id)
            .cloned()
            .unwrap_or_default(),
        engine: review_config
            .as_ref()
            .map(|config| config.provider.clone())
            .unwrap_or_else(|| "unknown".into()),
        model: review_config.and_then(|config| config.model),
    };
    if show_review {
        emit_voice_status(
            &app,
            Some(&session_id),
            "completed",
            Some("识别完成，请确认结果"),
        );
        set_review_shortcuts_background(&app, true);
    }
    diagnostic(
        &state,
        "info",
        "review.prepare_completed",
        Some(&session_id),
        serde_json::json!({
            "elapsedMs": started.elapsed().as_millis(),
            "textLength": review.text.chars().count(),
            "engine": review.engine,
            "model": review.model,
        }),
    );
    Ok(review)
}

#[tauri::command]
async fn commit_recording_review(
    app: AppHandle,
    state: tauri::State<'_, AppState>,
    session_id: String,
    text: String,
    ai: bool,
    llm_provider: Option<String>,
    llm_model: Option<String>,
) -> Result<InjectionState, AppError> {
    let segment = TranscriptSegment {
        text: text.clone(),
        stable: true,
        start_ms: 0,
        end_ms: 0,
        confidence: None,
    };
    let injection = state.injection.clone();
    let injection_session_id = session_id.clone();
    let result =
        match run_blocking(move || injection.commit_review(&injection_session_id, &segment)).await {
            Ok(result) => result,
            Err(error) => {
                restore_reviewing_after_insert_failure(&app, &state, &session_id);
                return Err(error);
            }
        };
    if result.state == "focus_lost" {
        restore_reviewing_after_insert_failure(&app, &state, &session_id);
        return Ok(result);
    }
    if let Some(session) = state.sessions.lock().get_mut(&session_id) {
        session.committed_text = text.clone();
        session.state = "inserted".into();
    }
    finalize_session(
        &app,
        &state,
        &session_id,
        Some(text),
        ai,
        llm_provider,
        llm_model,
    )?;
    emit_voice_status(&app, Some(&session_id), "completed", Some("已插入原输入框"));
    Ok(result)
}

fn restore_reviewing_after_insert_failure(app: &AppHandle, state: &AppState, session_id: &str) {
    if let Some(session) = state.sessions.lock().get_mut(session_id) {
        session.state = "reviewing".into();
    }
    set_review_shortcuts_background(app, true);
}

fn hotword_hit_deltas(matched: &[String]) -> HashMap<String, usize> {
    let mut counts = HashMap::<String, usize>::new();
    for word in matched {
        *counts.entry(word.clone()).or_default() += 1;
    }
    counts
}

fn rollback_session_hotword_hits(state: &AppState, session_id: &str) {
    let Some(matched) = state.session_hotwords.lock().remove(session_id) else {
        return;
    };
    if matched.is_empty() {
        return;
    }
    let counts = hotword_hit_deltas(&matched);
    let mut items = state.hotwords.lock();
    for item in items.iter_mut() {
        let Some(count) = counts.get(&item.word) else {
            continue;
        };
        item.hits = item.hits.saturating_sub(*count as i64).max(0);
        if let Err(error) = state.db.upsert_hotword(item) {
            diagnostic(
                state,
                "warn",
                "hotword.rollback_failed",
                Some(session_id),
                serde_json::json!({
                    "word": item.word,
                    "message": error.message,
                }),
            );
        }
    }
}

async fn cancel_recording_impl(
    app: &AppHandle,
    state: &AppState,
    session_id: &str,
) -> Result<(), AppError> {
    audio::stop_if_active(&state.audio, &session_id);
    state.streaming.cancel(&session_id);
    let injection = state.injection.clone();
    let injection_session_id = session_id.to_owned();
    let focus_error = run_blocking(move || injection.revert_all(&injection_session_id))
        .await?
        .filter(|result| result.state == "focus_lost")
        .map(|result| {
            AppError::new(
                "injection.focus_lost",
                result
                    .message
                    .unwrap_or_else(|| "无法撤回已上屏文字".into()),
            )
        });
    rollback_session_hotword_hits(state, session_id);
    state.sessions.lock().remove(session_id);
    state.session_configs.lock().remove(session_id);
    state.session_raw_text.lock().remove(session_id);
    state.injection.finish(session_id);
    clear_hold_release_pending(&mut state.hold_release_pending.lock(), Some(session_id));
    clear_active_session(&state.active_session, session_id);
    if let Some(bubble) = app.get_webview_window("bubble") {
        let _ = bubble.hide();
    }
    let _ = app.emit(
        "recording-cancelled",
        serde_json::json!({"sessionId":session_id}),
    );
    set_review_shortcuts_background(app, false);
    set_escape_shortcut_background(app, false);
    focus_error.map_or(Ok(()), Err)
}

#[tauri::command]
async fn cancel_recording(
    app: AppHandle,
    state: tauri::State<'_, AppState>,
    session_id: String,
) -> Result<(), AppError> {
    cancel_recording_impl(&app, &state, &session_id).await
}

fn cleanup_session(state: &AppState, session_id: &str) {
    rollback_session_hotword_hits(state, session_id);
    state.sessions.lock().remove(session_id);
    state.session_configs.lock().remove(session_id);
    state.session_raw_text.lock().remove(session_id);
    state.injection.finish(session_id);
    clear_hold_release_pending(&mut state.hold_release_pending.lock(), Some(session_id));
    clear_active_session(&state.active_session, session_id);
}

#[tauri::command]
fn recover_failed_session(app: AppHandle, state: tauri::State<'_, AppState>, session_id: String) {
    audio::stop_if_active(&state.audio, &session_id);
    state.streaming.cancel(&session_id);
    cleanup_session(&state, &session_id);
    set_review_shortcuts_background(&app, false);
    set_escape_shortcut_background(&app, false);
    let _ = app.emit(
        "recording-stopped",
        serde_json::json!({"sessionId":session_id}),
    );
}

pub(crate) fn handle_stream_segment(
    app: &AppHandle,
    session_id: &str,
    text: &str,
    stable: bool,
) -> Result<(), AppError> {
    let state = app.state::<AppState>();
    if !state.sessions.lock().contains_key(session_id) || text.is_empty() {
        return Ok(());
    }
    let target_title = state
        .sessions
        .lock()
        .get(session_id)
        .map(|session| session.target.title.clone())
        .unwrap_or_default();
    let (text, matched) = if stable {
        apply_session_hotwords(&state, text, "", &target_title)?
    } else {
        (text.to_owned(), Vec::new())
    };
    if stable {
        state
            .session_raw_text
            .lock()
            .entry(session_id.to_owned())
            .or_default()
            .push_str(text.as_str());
        if !matched.is_empty() {
            state
                .session_hotwords
                .lock()
                .entry(session_id.to_owned())
                .or_default()
                .extend(matched);
            let _ = app.emit("hotwords-changed", state.hotwords.lock().clone());
        }
    }
    let event_text = if stable {
        let committed = state
            .session_raw_text
            .lock()
            .get(session_id)
            .cloned()
            .unwrap_or_default();
        if let Some(session) = state.sessions.lock().get_mut(session_id) {
            session.committed_text = committed.clone();
            session.state = "streaming".into();
        }
        committed
    } else {
        text
    };
    let segment = TranscriptSegment {
        text: event_text,
        stable,
        start_ms: 0,
        end_ms: 0,
        confidence: None,
    };
    emit_transcript_event(
        app,
        if stable {
            "asr-stable-segment"
        } else {
            "asr-partial"
        },
        session_id,
        &segment,
    );
    Ok(())
}

#[tauri::command]
fn list_audio_devices() -> Result<Vec<AudioDevice>, AppError> {
    audio::list_devices()
}

#[tauri::command]
async fn inject_segment(
    app: AppHandle,
    state: tauri::State<'_, AppState>,
    session_id: String,
    segment: TranscriptSegment,
) -> Result<InjectionState, AppError> {
    let injection = state.injection.clone();
    let injection_session_id = session_id.clone();
    let result = run_blocking(move || injection.commit(&injection_session_id, &segment)).await?;
    if let Some(session) = state.sessions.lock().get_mut(&session_id) {
        session.committed_text = result.committed_text.clone();
        session.state = result.state.clone();
    }
    let _ = app.emit("injection-state", &result);
    Ok(result)
}

#[tauri::command]
async fn transcribe_final(
    app: AppHandle,
    state: tauri::State<'_, AppState>,
    session_id: String,
) -> Result<Option<TranscriptSegment>, AppError> {
    let started = std::time::Instant::now();
    emit_voice_status(&app, Some(&session_id), "recognizing", None);
    let config = state
        .session_configs
        .lock()
        .get(&session_id)
        .cloned()
        .ok_or_else(|| AppError::new("session.config", "录音会话缺少识别配置"))?;
    if config.provider == "sherpa-streaming" {
        let streamed_text = streamed_session_text(
            state
                .sessions
                .lock()
                .get(&session_id)
                .map(|session| session.committed_text.as_str()),
            state
                .session_raw_text
                .lock()
                .get(&session_id)
                .map(String::as_str),
        );
        diagnostic(
            &state,
            "info",
            "asr.final_used_stream",
            Some(&session_id),
            serde_json::json!({
                "elapsedMs": started.elapsed().as_millis(),
                "textLength": streamed_text.as_ref().map(|text| text.chars().count()).unwrap_or(0),
            }),
        );
        return Ok(streamed_text.map(|text| TranscriptSegment {
            text,
            stable: true,
            start_ms: 0,
            end_ms: 0,
            confidence: None,
        }));
    }
    let Some(samples) = audio::snapshot_pcm(&state.audio, &session_id)? else {
        diagnostic(
            &state,
            "warn",
            "asr.final_snapshot_empty",
            Some(&session_id),
            serde_json::json!({}),
        );
        return Ok(None);
    };
    let app_data = state.data_root.clone();
    diagnostic(
        &state,
        "info",
        "asr.final_started",
        Some(&session_id),
        serde_json::json!({
            "provider": config.provider,
            "model": config.model,
            "sampleCount": samples.len(),
        }),
    );
    let target_title = state
        .sessions
        .lock()
        .get(&session_id)
        .map(|session| session.target.title.clone())
        .unwrap_or_default();
    let result = tokio::time::timeout(
        FINAL_TRANSCRIPTION_TIMEOUT,
        asr::transcribe_samples(&samples, &config, &state.db, &app_data),
    )
    .await;
    let mut segment = match result {
        Err(_) => {
            let error = AppError::new(
                "asr.timeout",
                "最终识别超过 60 秒，已停止本次处理；请重试或切换流式识别模型",
            )
            .retryable();
            diagnostic(
                &state,
                "error",
                "asr.final_failed",
                Some(&session_id),
                serde_json::json!({
                    "elapsedMs": started.elapsed().as_millis(),
                    "code": error.code,
                    "message": error.message,
                }),
            );
            return Err(error);
        }
        Ok(Err(error)) => {
            diagnostic(
                &state,
                "error",
                "asr.final_failed",
                Some(&session_id),
                serde_json::json!({
                    "elapsedMs": started.elapsed().as_millis(),
                    "code": error.code,
                    "message": error.message,
                }),
            );
            return Err(error);
        }
        Ok(Ok(segment)) => segment,
    };
    emit_voice_status(&app, Some(&session_id), "transforming", None);
    let previous_raw = state
        .session_raw_text
        .lock()
        .get(&session_id)
        .cloned()
        .unwrap_or_default();
    let (text, matched) =
        apply_session_hotwords(&state, &segment.text, &previous_raw, &target_title)?;
    state
        .session_raw_text
        .lock()
        .insert(session_id.clone(), text.clone());
    if !matched.is_empty() {
        state
            .session_hotwords
            .lock()
            .entry(session_id.clone())
            .or_default()
            .extend(matched.clone());
        let _ = app.emit("hotwords-changed", state.hotwords.lock().clone());
    }
    segment.text = text;
    segment.stable = true;
    diagnostic(
        &state,
        "info",
        "asr.final_completed",
        Some(&session_id),
        serde_json::json!({
            "elapsedMs": started.elapsed().as_millis(),
            "textLength": segment.text.chars().count(),
            "hotwordMatches": matched.len(),
        }),
    );
    Ok(Some(segment))
}

#[tauri::command]
fn write_diagnostic_log(
    state: tauri::State<'_, AppState>,
    level: String,
    event: String,
    session_id: Option<String>,
    details: serde_json::Value,
) {
    diagnostic(&state, &level, &event, session_id.as_deref(), details);
}

#[tauri::command]
fn diagnostic_log_path(state: tauri::State<'_, AppState>) -> String {
    state.diagnostics.path().to_string_lossy().into_owned()
}

#[tauri::command]
fn restart_application(app: AppHandle) {
    app.restart();
}

#[tauri::command]
fn quit_application(app: AppHandle) {
    app.exit(0);
}

#[tauri::command]
async fn close_bubble(app: AppHandle, state: tauri::State<'_, AppState>) -> Result<(), AppError> {
    let active_session = state
        .active_session
        .lock()
        .clone()
        .filter(|session_id| state.sessions.lock().contains_key(session_id));
    diagnostic(
        &state,
        "info",
        "bubble.close_requested",
        active_session.as_deref(),
        serde_json::json!({
            "hadActiveSession": active_session.is_some(),
        }),
    );
    if let Some(bubble) = app.get_webview_window("bubble") {
        bubble
            .hide()
            .map_err(|error| AppError::new("bubble.hide", format!("关闭气泡失败：{error}")))?;
    }
    diagnostic(&state, "info", "bubble.closed", None, serde_json::json!({}));
    if let Some(session_id) = active_session {
        cancel_recording_impl(&app, &state, &session_id).await?;
    } else {
        *state.active_session.lock() = None;
    }
    Ok(())
}

#[tauri::command]
async fn replace_final_transcript(
    app: AppHandle,
    state: tauri::State<'_, AppState>,
    session_id: String,
    mut segment: TranscriptSegment,
) -> Result<InjectionState, AppError> {
    segment.stable = true;
    let injection = state.injection.clone();
    let injection_session_id = session_id.clone();
    let result =
        run_blocking(move || injection.replace_all(&injection_session_id, &segment)).await?;
    if let Some(session) = state.sessions.lock().get_mut(&session_id) {
        session.committed_text = result.committed_text.clone();
        session.state = result.state.clone();
    }
    let _ = app.emit("injection-state", &result);
    Ok(result)
}

#[tauri::command]
fn capture_focus_target() -> FocusTarget {
    injection::capture_foreground()
}

#[tauri::command]
async fn stream_llm(app: AppHandle, request: LlmRequest) -> Result<String, AppError> {
    let state = app.state::<AppState>();
    let db = state.db.clone();
    llm::stream(app, request, &db).await
}

#[tauri::command]
async fn test_llm_provider(
    state: tauri::State<'_, AppState>,
    request: ProviderTest,
) -> Result<(), AppError> {
    llm::test(
        &request.provider,
        &request.model,
        request.base_url.as_deref(),
        &state.db,
    )
    .await
}

#[tauri::command]
async fn list_llm_models(
    state: tauri::State<'_, AppState>,
    provider: String,
    base_url: Option<String>,
) -> Result<Vec<serde_json::Value>, AppError> {
    llm::list_models(&provider, base_url.as_deref(), &state.db).await
}

#[tauri::command]
async fn test_asr_provider(
    state: tauri::State<'_, AppState>,
    config: RecordingConfig,
) -> Result<(), AppError> {
    asr::test_provider(&config, &state.db, &state.data_root).await
}

#[tauri::command]
fn set_secret(
    state: tauri::State<'_, AppState>,
    name: String,
    value: String,
) -> Result<(), AppError> {
    ensure_data_writable(&state)?;
    secrets::set(&state.db, &name, &value)
}

#[tauri::command]
fn secret_configured(state: tauri::State<'_, AppState>, name: String) -> bool {
    secrets::configured(&state.db, &name)
}

#[tauri::command]
fn get_settings(state: tauri::State<'_, AppState>) -> Result<serde_json::Value, AppError> {
    Ok(serde_json::json!({"appSettings": state.settings.lock().clone()}))
}

#[tauri::command]
fn set_setting(
    app: AppHandle,
    state: tauri::State<'_, AppState>,
    key: String,
    value: serde_json::Value,
) -> Result<(), AppError> {
    ensure_data_writable(&state)?;
    if key == "appSettings" {
        let previous = hotkey_config(&state.settings.lock());
        let enabled = value
            .get("autoStart")
            .and_then(|item| item.as_bool())
            .unwrap_or(false);
        if let Err(error) = apply_autostart(&app, enabled) {
            diagnostic(
                &state,
                "warn",
                "autostart.apply_failed",
                None,
                serde_json::json!({
                    "enabled": enabled,
                    "error": error.message,
                }),
            );
        }
        let next = hotkey_config(&value);
        if previous != next {
            if let Err(error) = reregister_user_shortcuts(&app, &previous, &next) {
                let _ = reregister_user_shortcuts(&app, &next, &previous);
                return Err(error);
            }
        }
        *state.settings.lock() = value.clone();
        apply_retention(&state);
    }
    state.db.set_setting(&key, &value.to_string())?;
    let _ = app.emit(
        "settings-changed",
        serde_json::json!({"key": key, "value": value}),
    );
    Ok(())
}

#[tauri::command]
fn list_prompts(state: tauri::State<'_, AppState>) -> Result<Vec<PromptTemplate>, AppError> {
    state.db.list_prompts()
}

#[tauri::command]
fn upsert_prompt(
    app: AppHandle,
    state: tauri::State<'_, AppState>,
    prompt: PromptTemplate,
) -> Result<PromptTemplate, AppError> {
    ensure_data_writable(&state)?;
    let saved = state.db.upsert_prompt(&prompt)?;
    let _ = app.emit("prompts-changed", state.db.list_prompts()?);
    Ok(saved)
}

#[tauri::command]
fn remove_prompt(app: AppHandle, state: tauri::State<'_, AppState>, id: i64) -> Result<(), AppError> {
    ensure_data_writable(&state)?;
    state.db.delete_prompt(id)?;
    let _ = app.emit("prompts-changed", state.db.list_prompts()?);
    Ok(())
}

#[tauri::command]
fn clear_records(app: AppHandle, state: tauri::State<'_, AppState>) -> Result<(), AppError> {
    ensure_data_writable(&state)?;
    state.db.clear_records()?;
    let _ = app.emit("records-cleared", serde_json::json!({}));
    Ok(())
}

#[tauri::command]
fn list_hotwords(state: tauri::State<'_, AppState>) -> Result<Vec<Hotword>, AppError> {
    Ok(state.hotwords.lock().clone())
}

#[tauri::command]
fn upsert_hotword(
    app: AppHandle,
    state: tauri::State<'_, AppState>,
    hotword: Hotword,
) -> Result<(), AppError> {
    ensure_data_writable(&state)?;
    state.db.upsert_hotword(&hotword)?;
    let snapshot = {
        let mut items = state.hotwords.lock();
        if let Some(current) = items.iter_mut().find(|item| item.id == hotword.id) {
            *current = hotword;
        } else {
            items.push(hotword);
        }
        items.sort_by_key(|item| (std::cmp::Reverse(item.priority), std::cmp::Reverse(item.id)));
        items.clone()
    };
    let _ = app.emit("hotwords-changed", snapshot);
    Ok(())
}

#[tauri::command]
fn remove_hotword(
    app: AppHandle,
    state: tauri::State<'_, AppState>,
    id: i64,
) -> Result<(), AppError> {
    ensure_data_writable(&state)?;
    state.db.delete_hotword(id)?;
    let snapshot = {
        let mut items = state.hotwords.lock();
        items.retain(|item| item.id != id);
        items.clone()
    };
    let _ = app.emit("hotwords-changed", snapshot);
    Ok(())
}

#[tauri::command]
fn list_records(state: tauri::State<'_, AppState>) -> Result<Vec<RecordItem>, AppError> {
    state.db.list_records()
}

fn saved_recording_config(settings: &serde_json::Value) -> RecordingConfig {
    let provider = settings
        .get("asrProvider")
        .and_then(|v| v.as_str())
        .unwrap_or("openai")
        .to_owned();
    let local = matches!(
        provider.as_str(),
        "qwen3-asr" | "faster-whisper" | "sherpa-streaming"
    );
    RecordingConfig {
        device_id: settings
            .get("microphoneId")
            .and_then(|v| v.as_str())
            .map(str::to_owned),
        provider,
        model: settings
            .get(if local { "localModelPath" } else { "asrModel" })
            .and_then(|v| v.as_str())
            .map(str::to_owned),
        base_url: if local {
            None
        } else {
            settings
                .get("asrBaseUrl")
                .and_then(|v| v.as_str())
                .map(str::to_owned)
        },
        language: Some("zh".into()),
        compute_device: settings
            .get("computeDevice")
            .and_then(|v| v.as_str())
            .unwrap_or("auto")
            .to_owned(),
        hotwords: Vec::new(),
    }
}

#[tauri::command]
fn get_compute_status(state: tauri::State<'_, AppState>) -> Result<serde_json::Value, AppError> {
    let preference = state
        .settings
        .lock()
        .get("computeDevice")
        .and_then(|value| value.as_str())
        .unwrap_or("auto")
        .to_owned();
    Ok(local_asr::compute_status(&state.data_root, &preference))
}

fn apply_session_hotwords(
    state: &AppState,
    text: &str,
    previous_text: &str,
    app_name: &str,
) -> Result<(String, Vec<String>), AppError> {
    let settings = state.settings.lock().clone();
    if !settings
        .get("hotwordsEnabled")
        .and_then(|value| value.as_bool())
        .unwrap_or(true)
    {
        return Ok((text.to_owned(), Vec::new()));
    }
    let case_sensitive = settings
        .get("hotwordCaseSensitive")
        .and_then(|value| value.as_bool())
        .unwrap_or(false);
    let mut output = text.to_owned();
    let mut matched = Vec::new();
    let mut items = state.hotwords.lock();
    for item in items.iter_mut().filter(|item| item.enabled) {
        let applies = item
            .apps
            .iter()
            .any(|app| app == "全部应用" || app_name.contains(app));
        let haystack = if case_sensitive {
            output.clone()
        } else {
            output.to_lowercase()
        };
        let needle = if case_sensitive {
            item.word.clone()
        } else {
            item.word.to_lowercase()
        };
        if applies && haystack.contains(&needle) {
            let count = incremental_occurrences(
                &if case_sensitive {
                    previous_text.to_owned()
                } else {
                    previous_text.to_lowercase()
                },
                &if case_sensitive {
                    text.to_owned()
                } else {
                    text.to_lowercase()
                },
                &needle,
            );
            if let Some(replacement) = item
                .replacement
                .as_deref()
                .filter(|value| !value.is_empty())
            {
                if case_sensitive {
                    output = output.replace(&item.word, replacement);
                } else {
                    output = replace_case_insensitive(&output, &item.word, replacement);
                }
            }
            if count > 0 {
                item.hits += count as i64;
                item.last_used = chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
                state.db.upsert_hotword(item)?;
                matched.extend(std::iter::repeat_n(item.word.clone(), count));
            }
        }
    }
    Ok((output, matched))
}

fn replace_case_insensitive(haystack: &str, needle: &str, replacement: &str) -> String {
    if needle.is_empty() {
        return haystack.to_owned();
    }
    let lower = haystack.to_lowercase();
    let needle_lower = needle.to_lowercase();
    let mut output = String::new();
    let mut last = 0;
    let mut search = 0;
    while let Some(index) = lower[search..].find(&needle_lower) {
        let start = search + index;
        output.push_str(&haystack[last..start]);
        output.push_str(replacement);
        last = start + needle.len();
        search = last;
    }
    output.push_str(&haystack[last..]);
    output
}

fn incremental_occurrences(previous: &str, current: &str, needle: &str) -> usize {
    if current.starts_with(previous) {
        current
            .matches(needle)
            .count()
            .saturating_sub(previous.matches(needle).count())
    } else {
        current.matches(needle).count()
    }
}

#[cfg(test)]
mod state_tests {
    use super::{
        clear_active_session, hotword_hit_deltas, incremental_occurrences, is_autostart_launch,
        migrate_app_settings, reserve_active_session, review_should_finish_stream,
        session_holds_input_focus, should_hide_main_on_launch, streamed_session_text,
    };
    use parking_lot::Mutex;

    #[test]
    fn cancel_rolls_back_session_hotword_counts_without_going_negative() {
        let deltas = hotword_hit_deltas(&["飞书".into(), "飞书".into(), "语流".into()]);
        assert_eq!(deltas.get("飞书"), Some(&2));
        assert_eq!(deltas.get("语流"), Some(&1));
        assert_eq!(3_i64.saturating_sub(2).max(0), 1);
        assert_eq!(0_i64.saturating_sub(1).max(0), 0);
    }

    #[test]
    fn cumulative_transcripts_only_count_new_hotword_hits() {
        assert_eq!(incremental_occurrences("打开语", "打开语流", "语流"), 1);
        assert_eq!(incremental_occurrences("打开语流", "打开语流", "语流"), 0);
        assert_eq!(incremental_occurrences("上一段", "语流语流", "语流"), 2);
    }

    #[test]
    fn streaming_final_prefers_committed_text_over_raw_partials() {
        assert_eq!(
            streamed_session_text(Some("打开语流"), Some("打开")),
            Some("打开语流".into())
        );
        assert_eq!(
            streamed_session_text(Some("   "), Some("嗯")),
            Some("嗯".into())
        );
        assert_eq!(streamed_session_text(Some(""), Some("  ")), None);
    }

    #[test]
    fn autostart_hides_the_main_window_only_when_requested() {
        assert!(should_hide_main_on_launch(true, true));
        assert!(!should_hide_main_on_launch(true, false));
        assert!(!should_hide_main_on_launch(false, true));
        assert!(!should_hide_main_on_launch(false, false));
        assert!(is_autostart_launch(["VoiceFlow.exe", "--from-autostart"]));
        assert!(!is_autostart_launch(["VoiceFlow.exe"]));
    }

    #[test]
    fn voice_sessions_keep_the_original_input_focused() {
        assert!(session_holds_input_focus("capturing"));
        assert!(session_holds_input_focus("listening"));
        assert!(session_holds_input_focus("reviewing"));
        assert!(session_holds_input_focus("inserting"));
        assert!(session_holds_input_focus("finalizing"));
        assert!(!session_holds_input_focus("idle"));
        assert!(!session_holds_input_focus("inserted"));
    }

    #[test]
    fn legacy_settings_migrate_to_the_simplified_input_flow() {
        let (legacy, changed) = migrate_app_settings(serde_json::json!({
            "bubbleStyle": "C",
            "autoEndOnSilence": false,
            "liveInsert": false,
            "llmProvider": "claude",
            "llm": {
                "openai": { "autoFormat": false },
                "claude": { "autoFormat": true }
            }
        }));
        assert!(changed);
        assert_eq!(legacy["bubbleStyle"], "C");
        assert_eq!(legacy["directInsert"], false);
        assert_eq!(legacy["autoFormat"], true);
        assert_eq!(legacy["hideOnStart"], true);
        assert!(legacy.get("liveInsert").is_none());
        assert!(legacy["llm"]["claude"].get("autoFormat").is_none());
        assert_eq!(legacy["llm"]["openai"]["thinking"], false);
        assert_eq!(legacy["llm"]["claude"]["thinking"], false);

        let (current, changed) = migrate_app_settings(serde_json::json!({
            "bubbleStyle": "C",
            "autoEndOnSilence": false,
            "directInsert": true,
            "autoFormat": false
        }));
        assert!(changed);
        assert_eq!(current["directInsert"], true);
        assert_eq!(current["hotkeys"]["wake"], "Ctrl+Shift+Space");
        assert_eq!(current["hotkeys"]["ai"], "Ctrl+Alt+G");
        assert_eq!(current["hotwordsEnabled"], true);
        assert_eq!(current["hideOnStart"], true);

        let (again, changed_again) = migrate_app_settings(current.clone());
        assert!(!changed_again);
        assert_eq!(again["hotkeys"]["panel"], "Ctrl+Shift+V");

        let (kept_visible, kept_visible_changed) = migrate_app_settings(serde_json::json!({
            "bubbleStyle": "A",
            "autoEndOnSilence": true,
            "directInsert": true,
            "autoFormat": false,
            "hideOnStart": false,
            "hideOnStartUserSet": true,
            "hotkeys": {
                "wake": "Ctrl+Shift+Space",
                "panel": "Ctrl+Shift+V",
                "ai": "Ctrl+Alt+G",
                "mute": "Ctrl+Shift+M",
                "mode": "toggle",
                "holdMs": 220
            },
            "hotwordsEnabled": true
        }));
        assert_eq!(kept_visible["hideOnStart"], false);
        assert!(kept_visible.get("hideOnStartUserSet").is_none());
        let _ = kept_visible_changed;
    }

    #[test]
    fn review_finishes_a_live_stream_even_when_partial_text_exists() {
        assert!(review_should_finish_stream(true));
        assert!(!review_should_finish_stream(false));
    }

    #[test]
    fn openai_whisper_settings_migrate_to_realtime_transcribe() {
        let (migrated, changed) = migrate_app_settings(serde_json::json!({
            "directInsert": true,
            "autoFormat": false,
            "autoEndOnSilence": true,
            "asrProvider": "openai",
            "asrModel": "whisper-1"
        }));
        assert!(changed);
        assert_eq!(migrated["asrModel"], "gpt-4o-mini-transcribe");
        let (again, changed_again) = migrate_app_settings(migrated);
        assert!(!changed_again);
        assert_eq!(again["asrModel"], "gpt-4o-mini-transcribe");

        let (live, live_changed) = migrate_app_settings(serde_json::json!({
            "directInsert": true,
            "autoFormat": false,
            "autoEndOnSilence": true,
            "asrModel": "gpt-live-transcribe"
        }));
        assert!(live_changed);
        assert_eq!(live["asrModel"], "gpt-4o-mini-transcribe");

        let (kept, kept_changed) = migrate_app_settings(serde_json::json!({
            "directInsert": true,
            "autoFormat": false,
            "autoEndOnSilence": true,
            "asrProvider": "openai",
            "asrModel": "gpt-4o-transcribe"
        }));
        assert_eq!(kept["asrModel"], "gpt-4o-transcribe");
        let _ = kept_changed;

        let (local, local_changed) = migrate_app_settings(serde_json::json!({
            "directInsert": true,
            "autoFormat": false,
            "autoEndOnSilence": true,
            "asrProvider": "qwen3-asr",
            "asrModel": "whisper-1"
        }));
        assert_eq!(local["asrModel"], "whisper-1");
        let _ = local_changed;
    }

    #[test]
    fn active_session_is_reserved_once_and_only_cleared_by_its_owner() {
        let active = Mutex::new(None);
        reserve_active_session(&active, "session-a").expect("first session should reserve");
        assert_eq!(
            reserve_active_session(&active, "session-b")
                .expect_err("second session must be rejected")
                .code,
            "session.active"
        );
        assert!(!clear_active_session(&active, "session-b"));
        assert_eq!(active.lock().as_deref(), Some("session-a"));
        assert!(clear_active_session(&active, "session-a"));
        assert!(active.lock().is_none());
    }

    #[test]
    fn wake_treats_a_stale_active_id_without_session_as_idle() {
        use std::collections::HashMap;
        let mut sessions = HashMap::<String, ()>::new();
        sessions.insert("session-live".into(), ());
        let stale = Some("session-gone".to_owned());
        let live = Some("session-live".to_owned());
        assert!(stale
            .filter(|session_id| sessions.contains_key(session_id))
            .is_none());
        assert_eq!(
            live.filter(|session_id| sessions.contains_key(session_id))
                .as_deref(),
            Some("session-live")
        );
    }
}

#[tauri::command]
fn list_local_models(state: tauri::State<'_, AppState>) -> Result<Vec<LocalModelInfo>, AppError> {
    let root = state.data_root.join("models");
    local_models::list(&state.db, &root)
}

#[tauri::command]
fn local_models_directory(state: tauri::State<'_, AppState>) -> Result<String, AppError> {
    let root = state.data_root.join("models");
    std::fs::create_dir_all(&root).map_err(|e| AppError::new("model.directory", e.to_string()))?;
    Ok(root.to_string_lossy().into_owned())
}

#[tauri::command]
fn download_local_model(
    app: AppHandle,
    state: tauri::State<'_, AppState>,
    model_id: String,
) -> Result<(), AppError> {
    ensure_data_writable(&state)?;
    let model = local_models::find(&model_id)
        .ok_or_else(|| AppError::new("model.unknown", "未知的本地模型"))?;
    if state
        .model_downloads
        .lock()
        .get(&model_id)
        .is_some_and(|flag| flag.load(std::sync::atomic::Ordering::Relaxed))
    {
        return Ok(());
    }
    let app_data = state.data_root.clone();
    let running = Arc::new(std::sync::atomic::AtomicBool::new(true));
    state
        .model_downloads
        .lock()
        .insert(model_id, running.clone());
    let db = state.db.clone();
    let flags = state.model_downloads.clone();
    tauri::async_runtime::spawn(local_models::download(
        app, db, app_data, model, running, flags,
    ));
    Ok(())
}

#[tauri::command]
fn pause_local_model_download(
    state: tauri::State<'_, AppState>,
    model_id: String,
) -> Result<(), AppError> {
    let flag = state
        .model_downloads
        .lock()
        .get(&model_id)
        .cloned()
        .ok_or_else(|| AppError::new("model.not_downloading", "模型当前没有下载任务"))?;
    flag.store(false, std::sync::atomic::Ordering::Relaxed);
    Ok(())
}

#[tauri::command]
fn delete_local_model(state: tauri::State<'_, AppState>, model_id: String) -> Result<(), AppError> {
    ensure_data_writable(&state)?;
    let model = local_models::find(&model_id)
        .ok_or_else(|| AppError::new("model.unknown", "未知的本地模型"))?;
    if let Some(flag) = state.model_downloads.lock().get(&model_id) {
        flag.store(false, std::sync::atomic::Ordering::Relaxed);
    }
    let root = state.data_root.join("models");
    let target = root.join(model.id);
    local_asr::unload(&target);
    if target.exists() {
        std::fs::remove_dir_all(&target)
            .map_err(|e| AppError::new("model.delete", e.to_string()))?;
    }
    state.db.delete_model_state(model.id)?;
    Ok(())
}

#[tauri::command]
fn get_data_directory(state: tauri::State<'_, AppState>) -> data_directory::DataDirectoryInfo {
    data_directory::info(&state.data_root, &state.default_data_root)
}

#[tauri::command]
async fn choose_data_directory() -> Result<Option<data_directory::DataDirectoryChoice>, AppError> {
    let Some(folder) = rfd::AsyncFileDialog::new()
        .set_title("选择语流数据的保存位置")
        .pick_folder()
        .await
    else {
        return Ok(None);
    };
    data_directory::choice(folder.path().to_path_buf()).map(Some)
}

#[tauri::command]
async fn migrate_data_directory(
    state: tauri::State<'_, AppState>,
    parent_path: String,
) -> Result<data_directory::DataMigrationResult, AppError> {
    if state
        .data_migration_pending
        .compare_exchange(false, true, Ordering::Relaxed, Ordering::Relaxed)
        .is_err()
    {
        return Err(AppError::new(
            "data.migration_busy",
            "数据目录迁移已在进行或正在等待重启",
        ));
    }
    if state.active_session.lock().is_some() {
        state.data_migration_pending.store(false, Ordering::Relaxed);
        return Err(AppError::new(
            "data.recording_active",
            "录音进行中，结束录音后才能迁移数据目录",
        ));
    }
    if state
        .model_downloads
        .lock()
        .values()
        .any(|flag| flag.load(Ordering::Relaxed))
    {
        state.data_migration_pending.store(false, Ordering::Relaxed);
        return Err(AppError::new(
            "data.download_active",
            "本地模型正在下载，请暂停或等待下载完成后再迁移",
        ));
    }
    let db = state.db.clone();
    let current_root = state.data_root.clone();
    let default_root = state.default_data_root.clone();
    let location_file = state.data_location_file.clone();
    let result = run_blocking(move || {
        data_directory::migrate(
            &db,
            &current_root,
            &default_root,
            &location_file,
            &PathBuf::from(parent_path),
        )
    })
    .await;
    if result.is_err() {
        state.data_migration_pending.store(false, Ordering::Relaxed);
    }
    result
}

#[tauri::command]
fn quit_for_data_directory_restart(app: AppHandle) {
    app.exit(0);
}

const DISABLE_NATIVE_CONTEXT_MENU: &str = "document.addEventListener('keydown',function(event){if((event.ctrlKey||event.metaKey)&&!event.altKey&&String(event.key).toLowerCase()==='a'){var node=event.target instanceof Element?event.target:null;if(node&&node.closest('input,textarea'))return;event.preventDefault();}},true);";

fn apply_native_context_menu_block(window: &tauri::WebviewWindow) {
    let _ = window.eval(DISABLE_NATIVE_CONTEXT_MENU);
}

fn ensure_bubble_window(app: &AppHandle) -> Result<(), AppError> {
    if app.get_webview_window("bubble").is_some() {
        return Ok(());
    }
    let bubble_width = 620.0;
    let bubble_height = 180.0;
    let mut bubble_builder = WebviewWindowBuilder::new(
        app,
        "bubble",
        WebviewUrl::App("bubble.html".into()),
    )
    .title("语流录音")
    .inner_size(bubble_width, bubble_height)
    .decorations(false)
    .resizable(false)
    .transparent(true)
    .always_on_top(true)
    .skip_taskbar(true)
    .focused(false)
    .focusable(false)
    .shadow(false)
    .visible(false);
    if let Ok(Some(monitor)) = app.primary_monitor() {
        let area = monitor.work_area();
        let scale = monitor.scale_factor();
        let x = area.position.x as f64 / scale
            + (area.size.width as f64 / scale - bubble_width) / 2.0;
        let y = area.position.y as f64 / scale + area.size.height as f64 / scale
            - bubble_height
            - 24.0;
        bubble_builder = bubble_builder.position(x, y.max(0.0));
    }
    let window = bubble_builder
        .initialization_script(DISABLE_NATIVE_CONTEXT_MENU)
        .build()
        .map_err(|error| AppError::new("bubble.create", format!("创建气泡窗口失败：{error}")))?;
    apply_native_context_menu_block(&window);
    Ok(())
}

fn voice_session_owns_foreground(app: &AppHandle) -> bool {
    let Some(state) = app.try_state::<AppState>() else {
        return false;
    };
    let Some(session_id) = state.active_session.lock().clone() else {
        return false;
    };
    let session_state = state
        .sessions
        .lock()
        .get(&session_id)
        .map(|session| session.state.clone());
    session_state
        .as_deref()
        .is_some_and(session_holds_input_focus)
}

fn session_holds_input_focus(state: &str) -> bool {
    matches!(
        state,
        "capturing" | "listening" | "reviewing" | "inserting" | "finalizing"
    )
}

fn is_autostart_launch<I, S>(args: I) -> bool
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    args.into_iter()
        .any(|arg| arg.as_ref() == AUTOSTART_ARG)
}

fn should_hide_main_on_launch(hide_on_start: bool, launched_from_autostart: bool) -> bool {
    hide_on_start && launched_from_autostart
}

fn show_main(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.set_skip_taskbar(false);
        let _ = window.show();
        let _ = window.unminimize();
        if !voice_session_owns_foreground(app) {
            let _ = window.set_focus();
        }
    }
}

#[tauri::command]
fn hide_main_window(app: AppHandle) {
    hide_main(&app);
}

fn hide_main(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.set_skip_taskbar(true);
        let _ = window.hide();
    }
}

fn toggle_main(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        if window.is_visible().unwrap_or(false) {
            hide_main(app);
        } else {
            show_main(app);
        }
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _, _| {
            show_main(app)
        }))
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            Some(vec![AUTOSTART_ARG]),
        ))
        .setup(|app| {
            let default_data_root = app.path().app_data_dir()?;
            let data_location_file = app.path().app_config_dir()?.join("data-location.json");
            let data_root = data_directory::resolve(&default_data_root, &data_location_file)?;
            let db_path = data_root.join("yuliu.sqlite3");
            let db = Database::open(&db_path)?;
            db.recover_interrupted_model_downloads()?;
            db.seed_prompts_if_empty()?;
            let persisted = db.get_settings()?;
            let settings = persisted.get("appSettings").cloned().unwrap_or_default();
            let (settings, settings_migrated) = migrate_app_settings(settings);
            if settings_migrated {
                db.set_setting("appSettings", &settings.to_string())?;
            }
            let _ = db.delete_expired_records(
                settings
                    .get("retentionDays")
                    .and_then(|value| value.as_i64())
                    .unwrap_or(30),
            );
            let hide_on_start = settings
                .get("hideOnStart")
                .and_then(|value| value.as_bool())
                .unwrap_or(true);
            let auto_start = settings
                .get("autoStart")
                .and_then(|value| value.as_bool())
                .unwrap_or(false);
            let launched_from_autostart = is_autostart_launch(std::env::args());
            let startup_hotkeys = hotkey_config(&settings);
            let hotwords = db.list_hotwords()?;
            let diagnostics = DiagnosticLog::new(&data_root);
            let state = AppState {
                db,
                data_root,
                default_data_root,
                data_location_file,
                data_migration_pending: AtomicBool::new(false),
                audio: AudioManager::default(),
                injection: InjectionManager::default(),
                sessions: Mutex::new(HashMap::new()),
                active_session: Arc::new(Mutex::new(None)),
                model_downloads: Arc::new(Mutex::new(HashMap::new())),
                session_configs: Mutex::new(HashMap::new()),
                session_hotwords: Mutex::new(HashMap::new()),
                session_raw_text: Mutex::new(HashMap::new()),
                settings: Mutex::new(settings),
                hotwords: Mutex::new(hotwords),
                streaming: streaming_asr::StreamingManager::default(),
                diagnostics,
                wake_pressed_at: Mutex::new(None),
                hold_release_pending: Mutex::new(None),
            };
            app.manage(state);
            app.state::<AppState>().diagnostics.write(
                "info",
                "application.started",
                None,
                serde_json::json!({
                    "version": app.package_info().version.to_string(),
                    "logPath": app.state::<AppState>().diagnostics.path(),
                }),
            );

            let show_item = MenuItem::with_id(app, "show", "显示语流", true, None::<&str>)?;
            let quit_item = MenuItem::with_id(app, "quit", "退出", true, None::<&str>)?;
            let tray_menu = Menu::with_items(app, &[&show_item, &quit_item])?;
            let mut tray = TrayIconBuilder::with_id("main")
                .tooltip(tray_tooltip(&startup_hotkeys.wake))
                .menu(&tray_menu)
                .show_menu_on_left_click(false)
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "show" => show_main(app),
                    "quit" => app.exit(0),
                    _ => {}
                })
                .on_tray_icon_event(|tray, event| {
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } = event
                    {
                        show_main(tray.app_handle());
                    }
                });
            if let Some(icon) = app.default_window_icon() {
                tray = tray.icon(icon.clone());
            }
            tray.build(app)?;

            app.handle().plugin(
                tauri_plugin_global_shortcut::Builder::new()
                    .with_handler(|app, shortcut, event| {
                        let pressed = event.state() == ShortcutState::Pressed;
                        if event.state() != ShortcutState::Pressed
                            && event.state() != ShortcutState::Released
                        {
                            return;
                        }
                        let config = hotkey_config(&app.state::<AppState>().settings.lock());
                        let Some(action) = classify_shortcut(&shortcut.to_string(), &config) else {
                            return;
                        };
                        handle_classified_shortcut(app, action, pressed);
                    })
                    .build(),
            )?;
            register_user_shortcuts_with_policy(app.handle(), &startup_hotkeys, false)?;
            let _ = apply_autostart(app.handle(), auto_start);
            if should_hide_main_on_launch(hide_on_start, launched_from_autostart) {
                hide_main(app.handle());
            } else {
                show_main(app.handle());
            }
            let warmup_app = app.handle().clone();
            std::thread::spawn(move || {
                std::thread::sleep(Duration::from_millis(400));
                let _ = ensure_bubble_window(&warmup_app);
            });
            Ok(())
        })
        .on_page_load(|webview, payload| {
            if payload.event() == PageLoadEvent::Finished {
                let _ = webview.eval(DISABLE_NATIVE_CONTEXT_MENU);
            }
        })
        .on_window_event(|window, event| {
            if window.label() == "main" {
                if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                    api.prevent_close();
                    hide_main(window.app_handle());
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            start_recording,
            stop_recording,
            stop_recording_for_review,
            commit_recording_review,
            cancel_recording,
            recover_failed_session,
            list_audio_devices,
            inject_segment,
            replace_final_transcript,
            transcribe_final,
            capture_focus_target,
            stream_llm,
            test_llm_provider,
            list_llm_models,
            test_asr_provider,
            get_compute_status,
            set_secret,
            secret_configured,
            get_settings,
            set_setting,
            list_hotwords,
            upsert_hotword,
            remove_hotword,
            list_prompts,
            upsert_prompt,
            remove_prompt,
            list_records,
            clear_records,
            list_local_models,
            local_models_directory,
            download_local_model,
            pause_local_model_download,
            delete_local_model,
            get_data_directory,
            choose_data_directory,
            migrate_data_directory,
            quit_for_data_directory_restart,
            write_diagnostic_log,
            diagnostic_log_path,
            restart_application,
            quit_application,
            hide_main_window,
            close_bubble
        ])
        .run(tauri::generate_context!())
        .expect("failed to run VoiceFlow");
}
