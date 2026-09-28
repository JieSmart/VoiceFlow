use crate::streaming_asr::{AudioChunk, StreamMessage};
use crate::{error::AppError, models::AudioDevice};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use parking_lot::Mutex;
use serde::Serialize;
use std::{
    collections::HashMap,
    sync::{mpsc, Arc},
    thread,
    time::{Duration, Instant},
};
use tauri::{AppHandle, Emitter};
use tokio::sync::mpsc as tokio_mpsc;

pub struct Capture {
    pub stop: mpsc::Sender<()>,
    pub join: Option<thread::JoinHandle<()>>,
    pub samples: Arc<Mutex<Vec<f32>>>,
    pub sample_rate: u32,
    pub channels: u16,
}
#[derive(Default)]
pub struct AudioManager(pub Mutex<HashMap<String, Capture>>);

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct LevelEvent {
    session_id: String,
    /// Display-friendly level mapped from roughly -55 dB to 0 dB.
    level: f32,
    /// Unscaled RMS used by the frontend voice activity detector.
    rms: f32,
}

pub fn list_devices() -> Result<Vec<AudioDevice>, AppError> {
    let host = cpal::default_host();
    let default = host.default_input_device().and_then(|d| d.name().ok());
    let devices = host
        .input_devices()
        .map_err(|e| AppError::new("audio.devices", e.to_string()))?;
    Ok(devices
        .filter_map(|d| {
            d.name().ok().map(|name| AudioDevice {
                id: name.clone(),
                is_default: default.as_deref() == Some(name.as_str()),
                name,
            })
        })
        .collect())
}

pub fn resolve_input_device(
    host: &cpal::Host,
    selected: Option<&str>,
) -> Result<cpal::Device, AppError> {
    let devices: Vec<_> = host
        .input_devices()
        .map_err(|e| AppError::new("audio.devices", e.to_string()))?
        .collect();
    let names: Vec<String> = devices
        .iter()
        .filter_map(|device| device.name().ok())
        .collect();
    let name_refs: Vec<&str> = names.iter().map(String::as_str).collect();
    let matched_index = resolve_saved_microphone(selected, &name_refs).and_then(|name| {
        devices
            .iter()
            .position(|device| device.name().ok().as_deref() == Some(name.as_str()))
    });
    let device = matched_index
        .and_then(|index| devices.into_iter().nth(index))
        .or_else(|| host.default_input_device())
        .ok_or_else(|| AppError::new("audio.no_device", "未找到可用麦克风"))?;
    Ok(device)
}

#[cfg(test)]
mod device_selection_tests {
    use super::resolve_saved_microphone;

    #[test]
    fn prefers_saved_device_name_over_enumeration_index() {
        let names = ["Headset Mic", "Built-in Mic", "USB Mic"];
        assert_eq!(
            resolve_saved_microphone(Some("USB Mic"), &names).as_deref(),
            Some("USB Mic")
        );
        assert_eq!(
            resolve_saved_microphone(Some("1"), &names).as_deref(),
            Some("Built-in Mic")
        );
        assert_eq!(resolve_saved_microphone(Some("missing"), &names), None);
        assert_eq!(resolve_saved_microphone(None, &names), None);
    }
}

pub fn resolve_saved_microphone(selected: Option<&str>, names: &[&str]) -> Option<String> {
    let requested = selected.map(str::trim).filter(|value| !value.is_empty())?;
    names
        .iter()
        .find(|name| **name == requested)
        .map(|name| (*name).to_owned())
        .or_else(|| {
            requested
                .parse::<usize>()
                .ok()
                .and_then(|index| names.get(index).map(|name| (*name).to_owned()))
        })
}

pub fn start(
    app: AppHandle,
    manager: &AudioManager,
    session_id: &str,
    device_id: Option<&str>,
    stream_sender: Option<tokio_mpsc::Sender<StreamMessage>>,
) -> Result<(), AppError> {
    let samples = Arc::new(Mutex::new(Vec::<f32>::new()));
    let sid = session_id.to_owned();
    let selected = device_id.map(str::to_owned);
    let sink = samples.clone();
    let (stop_tx, stop_rx) = mpsc::channel();
    let (ready_tx, ready_rx) = mpsc::sync_channel(1);
    let join = thread::spawn(move || {
        let result = (|| -> Result<(cpal::Stream, u32, u16), AppError> {
            let host = cpal::default_host();
            let device = resolve_input_device(&host, selected.as_deref())?;
            let supported = device
                .default_input_config()
                .map_err(|e| AppError::new("audio.config", e.to_string()))?;
            let sample_rate = supported.sample_rate().0;
            let channels = supported.channels();
            let config: cpal::StreamConfig = supported.clone().into();
            macro_rules! stream_for {
                ($ty:ty, $convert:expr) => {{
                    let event_app = app.clone(); let event_sid = sid.clone(); let event_sink = sink.clone();
                    let asr_sender = stream_sender.clone();
                    let error_app = app.clone(); let error_sid = sid.clone();
                    let mut meter_sum = 0.0_f64;
                    let mut meter_samples = 0_usize;
                    let mut meter_dc = 0.0_f64;
                    let mut last_meter_emit = Instant::now();
                    device.build_input_stream(&config, move |data: &[$ty], _| {
                        let values: Vec<f32> = data.iter().map($convert).collect();
                        // Keep recorded/streamed PCM untouched, but remove DC and
                        // very-low-frequency rumble from the level used by VAD.
                        for value in &values {
                            let value = *value as f64;
                            meter_dc += (value - meter_dc) * 0.002;
                            let centered = value - meter_dc;
                            meter_sum += centered * centered;
                        }
                        meter_samples += values.len();
                        if let Some(sender) = &asr_sender {
                            let _ = sender.try_send(StreamMessage::Audio(AudioChunk {
                                samples: values.clone(),
                                sample_rate,
                                channels,
                            }));
                        }
                        event_sink.lock().extend_from_slice(&values);
                        if last_meter_emit.elapsed() >= Duration::from_millis(33) {
                            let rms = (meter_sum / meter_samples.max(1) as f64).sqrt().min(1.0) as f32;
                            let db = 20.0 * rms.max(0.000_01).log10();
                            let level = ((db + 55.0) / 45.0).clamp(0.0, 1.0);
                            let _ = event_app.emit("audio-level", LevelEvent {
                                session_id: event_sid.clone(),
                                level,
                                rms,
                            });
                            meter_sum = 0.0;
                            meter_samples = 0;
                            last_meter_emit = Instant::now();
                        }
                    }, move |e| { let _ = error_app.emit("session-error", serde_json::json!({"sessionId":error_sid,"code":"audio.stream","message":e.to_string()})); }, None)
                }};
            }
            let stream = match supported.sample_format() {
                cpal::SampleFormat::F32 => stream_for!(f32, |v: &f32| *v),
                cpal::SampleFormat::I16 => stream_for!(i16, |v: &i16| *v as f32 / i16::MAX as f32),
                cpal::SampleFormat::U16 => {
                    stream_for!(u16, |v: &u16| (*v as f32 / u16::MAX as f32) * 2.0 - 1.0)
                }
                f => {
                    return Err(AppError::new(
                        "audio.format",
                        format!("不支持的采样格式 {f:?}"),
                    ))
                }
            }
            .map_err(|e| AppError::new("audio.stream", e.to_string()))?;
            stream
                .play()
                .map_err(|e| AppError::new("audio.start", e.to_string()))?;
            Ok((stream, sample_rate, channels))
        })();
        match result {
            Ok((stream, sample_rate, channels)) => {
                let _ = ready_tx.send(Ok((sample_rate, channels)));
                let _ = stop_rx.recv();
                drop(stream);
            }
            Err(error) => {
                let _ = ready_tx.send(Err(error));
            }
        }
    });
    let (sample_rate, channels) = ready_rx
        .recv()
        .map_err(|_| AppError::new("audio.thread", "录音线程启动失败"))??;
    manager.0.lock().insert(
        session_id.to_owned(),
        Capture {
            stop: stop_tx,
            join: Some(join),
            samples,
            sample_rate,
            channels,
        },
    );
    Ok(())
}

pub fn snapshot_pcm(manager: &AudioManager, session_id: &str) -> Result<Option<Vec<f32>>, AppError> {
    let captures = manager.0.lock();
    let capture = captures
        .get(session_id)
        .ok_or_else(|| AppError::new("session.missing", "录音会话不存在"))?;
    let raw = capture.samples.lock().clone();
    if raw.is_empty() {
        return Ok(None);
    }
    let rate = capture.sample_rate;
    let channels = capture.channels;
    drop(captures);
    Ok(Some(resample_to_16k_mono(&raw, rate, channels)))
}

fn resample_to_16k_mono(raw: &[f32], sample_rate: u32, channels: u16) -> Vec<f32> {
    let channels = channels.max(1) as usize;
    let mono: Vec<f32> = raw
        .chunks(channels)
        .map(|frame| frame.iter().sum::<f32>() / frame.len().max(1) as f32)
        .collect();
    if sample_rate == 16_000 || mono.is_empty() {
        return mono;
    }
    let ratio = sample_rate as f64 / 16_000.0;
    let out_len = (mono.len() as f64 / ratio) as usize;
    (0..out_len)
        .map(|i| {
            let pos = i as f64 * ratio;
            let a = pos.floor() as usize;
            let b = (a + 1).min(mono.len().saturating_sub(1));
            let t = (pos - a as f64) as f32;
            mono.get(a).copied().unwrap_or(0.0) * (1.0 - t)
                + mono.get(b).copied().unwrap_or(0.0) * t
        })
        .collect()
}

pub fn stop_if_active(manager: &AudioManager, session_id: &str) -> bool {
    let Some(mut capture) = manager.0.lock().remove(session_id) else {
        return false;
    };
    let _ = capture.stop.send(());
    if let Some(join) = capture.join.take() {
        // Dropping a CPAL stream can occasionally block inside the Windows audio
        // driver. The capture owns all of its buffers, so after signalling it to
        // stop we must not hold the review IPC open waiting for that driver call.
        // Reap an already-finished thread; otherwise detach it and let it exit.
        if join.is_finished() {
            let _ = join.join();
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::{snapshot_pcm, stop_if_active, AudioManager, Capture};
    use parking_lot::Mutex;
    use std::{
        sync::{mpsc, Arc},
        thread,
        time::{Duration, Instant},
    };

    #[test]
    fn snapshot_pcm_downmixes_and_resamples_to_16k_mono() {
        let manager = AudioManager::default();
        let (stop_tx, stop_rx) = mpsc::channel();
        let join = thread::spawn(move || {
            let _ = stop_rx.recv();
        });
        manager.0.lock().insert(
            "session".into(),
            Capture {
                stop: stop_tx,
                join: Some(join),
                samples: Arc::new(Mutex::new((0..480).flat_map(|_| [1.0, -1.0]).collect())),
                sample_rate: 48_000,
                channels: 2,
            },
        );
        let samples = snapshot_pcm(&manager, "session")
            .expect("snapshot")
            .expect("pcm");
        assert_eq!(samples.len(), 160);
        let _ = stop_if_active(&manager, "session");
    }

    #[test]
    fn stop_does_not_wait_for_a_slow_audio_driver_shutdown() {
        let manager = AudioManager::default();
        let (stop_tx, stop_rx) = mpsc::channel();
        let join = thread::spawn(move || {
            let _ = stop_rx.recv();
            thread::sleep(Duration::from_millis(300));
        });
        manager.0.lock().insert(
            "session".into(),
            Capture {
                stop: stop_tx,
                join: Some(join),
                samples: Arc::new(Mutex::new(Vec::new())),
                sample_rate: 16_000,
                channels: 1,
            },
        );

        let started = Instant::now();
        assert!(stop_if_active(&manager, "session"));

        assert!(started.elapsed() < Duration::from_millis(100));
        assert!(!manager.0.lock().contains_key("session"));
    }

    #[test]
    fn stop_if_active_is_an_atomic_noop_for_an_already_stopped_capture() {
        let manager = AudioManager::default();

        assert!(!stop_if_active(&manager, "missing"));
    }
}
