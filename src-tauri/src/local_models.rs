use crate::{
    db::Database,
    error::AppError,
    local_asr,
    models::{LocalModelInfo, LocalModelProgress},
};
use bzip2::read::BzDecoder;
use futures_util::StreamExt;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    fs::{File, OpenOptions},
    io::{Read, Write},
    path::{Component, Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};
use tauri::{AppHandle, Emitter};

const RUNTIME_URL: &str = "https://github.com/k2-fsa/sherpa-onnx/releases/download/v1.13.7/sherpa-onnx-v1.13.7-win-x64-shared-MT-Release-no-tts-lib.tar.bz2";
const RUNTIME_SIZE: u64 = 7_378_634;
const RUNTIME_SHA256: &str = "ebbcb8e6ef5ba4fb2444810fb7cc8dc0154e66f84a2101bf7c5cbcc16ce497a9";
const CUDA_RUNTIME_URL: &str = "https://github.com/k2-fsa/sherpa-onnx/releases/download/v1.13.7/sherpa-onnx-v1.13.7-cuda-12.x-cudnn-9.x-onnxruntime1.27.1-win-x64-cuda.tar.bz2";
const CUDA_RUNTIME_SIZE: u64 = 375_601_142;
const CUDA_RUNTIME_SHA256: &str = "3aa63e4dd6a394a7ee3e8b170a80df49c27b6cf0e52cea4fa77fc51431f3bc27";
const DOWNLOAD_USER_AGENT: &str = "VoiceInputX/0.1";
static RUNTIME_INSTALL: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

#[derive(Clone)]
pub struct CatalogModel {
    pub id: &'static str,
    pub engine: &'static str,
    pub name: &'static str,
    pub size: &'static str,
    pub languages: &'static str,
    pub speed: &'static str,
    pub description: &'static str,
    pub source: ModelSource,
    pub repository: &'static str,
    pub files: &'static [&'static str],
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ModelSource {
    HuggingFace,
    ModelScope,
}

#[derive(Deserialize)]
struct Repository {
    siblings: Vec<RepositoryFile>,
}

#[derive(Deserialize)]
struct RepositoryFile {
    rfilename: String,
    size: Option<u64>,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct ModelScopeResponse {
    data: ModelScopeData,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct ModelScopeData {
    files: Vec<ModelScopeFile>,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct ModelScopeFile {
    path: String,
    size: u64,
}

enum DownloadOutcome {
    Complete,
    Paused,
}

pub type DownloadFlags = Arc<parking_lot::Mutex<HashMap<String, Arc<AtomicBool>>>>;

pub fn catalog() -> Vec<CatalogModel> {
    vec![
        CatalogModel {
            id: "sherpa-streaming-zipformer-zh-en",
            engine: "sherpa-streaming",
            name: "Sherpa Zipformer 中英流式",
            size: "约 190 MB",
            languages: "中文 / 英文",
            speed: "实时",
            description: "真正的本地增量识别，边说边出字",
            source: ModelSource::HuggingFace,
            repository: "csukuangfj/sherpa-onnx-streaming-zipformer-bilingual-zh-en-2023-02-20",
            files: &[
                "encoder-epoch-99-avg-1.int8.onnx",
                "decoder-epoch-99-avg-1.int8.onnx",
                "joiner-epoch-99-avg-1.int8.onnx",
                "tokens.txt",
            ],
        },
        CatalogModel {
            id: "qwen3-asr-1.7b",
            engine: "qwen3-asr",
            name: "Qwen3-ASR 1.7B INT8",
            size: "约 2.4 GB",
            languages: "30 种语言 / 22 种中文方言",
            speed: "中",
            description: "高准确率多语言与中文方言识别",
            source: ModelSource::ModelScope,
            repository: "zengshuishui/Qwen3-ASR-onnx",
            files: &[
                "model_1.7B/conv_frontend.onnx",
                "model_1.7B/encoder.int8.onnx",
                "model_1.7B/decoder.int8.onnx",
                "tokenizer/merges.txt",
                "tokenizer/tokenizer_config.json",
                "tokenizer/vocab.json",
            ],
        },
        CatalogModel {
            id: "faster-whisper-medium",
            engine: "faster-whisper",
            name: "Whisper medium INT8",
            size: "约 947 MB",
            languages: "99 种语言",
            speed: "中快",
            description: "准确率与资源占用均衡",
            source: ModelSource::HuggingFace,
            repository: "csukuangfj/sherpa-onnx-whisper-medium",
            files: &[
                "medium-encoder.int8.onnx",
                "medium-decoder.int8.onnx",
                "medium-tokens.txt",
            ],
        },
        CatalogModel {
            id: "faster-whisper-large-v3",
            engine: "faster-whisper",
            name: "Whisper large-v3 INT8",
            size: "约 1.78 GB",
            languages: "99 种语言",
            speed: "中",
            description: "多语言高准确率模型",
            source: ModelSource::HuggingFace,
            repository: "csukuangfj/sherpa-onnx-whisper-large-v3",
            files: &[
                "large-v3-encoder.int8.onnx",
                "large-v3-decoder.int8.onnx",
                "large-v3-tokens.txt",
            ],
        },
    ]
}

pub fn find(id: &str) -> Option<CatalogModel> {
    catalog().into_iter().find(|model| model.id == id)
}

pub fn list(db: &Database, root: &Path) -> Result<Vec<LocalModelInfo>, AppError> {
    let states: HashMap<_, _> = db
        .list_models()?
        .into_iter()
        .map(|state| (state.id.clone(), state))
        .collect();
    Ok(catalog()
        .into_iter()
        .map(|model| {
            let state = states.get(model.id);
            let expected_path = root.join(model.id);
            let runtime_installed = root
                .parent()
                .is_some_and(|app_data| runtime_ready(&local_asr::runtime_directory(app_data)));
            let installed = runtime_installed
                && state.is_some_and(|value| value.status == "done")
                && model_ready(&model, &expected_path);
            LocalModelInfo {
                id: model.id.into(),
                engine: model.engine.into(),
                name: model.name.into(),
                size: model.size.into(),
                languages: model.languages.into(),
                speed: model.speed.into(),
                description: model.description.into(),
                status: if installed {
                    "done".into()
                } else {
                    state
                        .map(|value| value.status.clone())
                        .filter(|value| value != "done")
                        .unwrap_or_else(|| "idle".into())
                },
                progress: if installed {
                    100
                } else {
                    state.map(|value| value.progress).unwrap_or(0)
                },
                path: if installed {
                    Some(expected_path.to_string_lossy().into_owned())
                } else {
                    state.and_then(|value| value.path.clone())
                },
                message: None,
            }
        })
        .collect())
}

fn model_ready(model: &CatalogModel, root: &Path) -> bool {
    model.files.iter().all(|name| root.join(name).is_file())
}

fn emit(
    app: &AppHandle,
    id: &str,
    status: &str,
    progress: u8,
    path: Option<&Path>,
    message: Option<String>,
    downloaded_bytes: Option<u64>,
    total_bytes: Option<u64>,
    bytes_per_second: Option<u64>,
) {
    let _ = app.emit(
        "local-model-progress",
        LocalModelProgress {
            id: id.into(),
            status: status.into(),
            progress,
            path: path.map(|value| value.to_string_lossy().into_owned()),
            message,
            downloaded_bytes,
            total_bytes,
            bytes_per_second,
        },
    );
}

fn safe_relative_path(name: &str) -> Result<PathBuf, AppError> {
    let mut relative = PathBuf::new();
    for component in Path::new(name).components() {
        match component {
            Component::Normal(value) => relative.push(value),
            _ => return Err(AppError::new("model.path", "模型仓库包含不安全的文件路径")),
        }
    }
    if relative.as_os_str().is_empty() {
        return Err(AppError::new("model.path", "模型仓库包含空文件路径"));
    }
    Ok(relative)
}

fn file_url(model: &CatalogModel, filename: &str) -> Result<url::Url, AppError> {
    if model.source == ModelSource::ModelScope {
        let mut value = url::Url::parse(&format!(
            "https://www.modelscope.cn/api/v1/models/{}/repo",
            model.repository
        ))
        .map_err(|e| AppError::new("model.url", e.to_string()))?;
        value
            .query_pairs_mut()
            .append_pair("Revision", "master")
            .append_pair("FilePath", filename);
        return Ok(value);
    }
    let mut value = url::Url::parse("https://huggingface.co")
        .map_err(|e| AppError::new("model.url", e.to_string()))?;
    {
        let mut segments = value
            .path_segments_mut()
            .map_err(|_| AppError::new("model.url", "无法构造模型下载地址"))?;
        for segment in model.repository.split('/') {
            segments.push(segment);
        }
        segments.push("resolve").push("main");
        for segment in filename.split('/') {
            segments.push(segment);
        }
    }
    Ok(value)
}

pub async fn download(
    app: AppHandle,
    db: Database,
    app_data: PathBuf,
    model: CatalogModel,
    running: Arc<AtomicBool>,
    flags: DownloadFlags,
) {
    let result = download_inner(&app, &db, &app_data, &model, &running).await;
    flags.lock().remove(model.id);
    if let Err(error) = result {
        let target = app_data.join("models").join(model.id);
        let _ = db.set_model_state(
            model.id,
            model.engine,
            model.name,
            Some(&target.to_string_lossy()),
            "error",
            0,
        );
        emit(
            &app,
            model.id,
            "error",
            0,
            Some(&target),
            Some(error.message),
            None,
            None,
            None,
        );
    }
}

async fn download_inner(
    app: &AppHandle,
    db: &Database,
    app_data: &Path,
    model: &CatalogModel,
    running: &AtomicBool,
) -> Result<(), AppError> {
    let target = app_data.join("models").join(model.id);
    std::fs::create_dir_all(&target)
        .map_err(|e| AppError::new("model.directory", e.to_string()))?;
    set_state(
        db,
        app,
        model,
        &target,
        "downloading",
        0,
        Some("正在准备推理运行时"),
        None,
        None,
        None,
    )?;
    if !ensure_runtime(app, model, &target, app_data, running).await? {
        set_state(
            db,
            app,
            model,
            &target,
            "paused",
            0,
            Some("下载已暂停"),
            None,
            None,
            None,
        )?;
        return Ok(());
    }

    let client = download_client()?;
    let metadata_url = match model.source {
        ModelSource::HuggingFace => format!(
            "https://huggingface.co/api/models/{}/revision/main?blobs=true",
            model.repository
        ),
        ModelSource::ModelScope => format!(
            "https://www.modelscope.cn/api/v1/models/{}/repo/files?Revision=master&Recursive=true",
            model.repository
        ),
    };
    let metadata = client
        .get(metadata_url)
        .send()
        .await
        .map_err(|e| AppError::new("model.network", e.to_string()).retryable())?;
    if !metadata.status().is_success() {
        return Err(AppError::new(
            "model.catalog",
            format!("模型清单请求失败：{}", metadata.status()),
        ));
    }
    let repository = match model.source {
        ModelSource::HuggingFace => metadata
            .json::<Repository>()
            .await
            .map_err(|e| AppError::new("model.catalog", e.to_string()))?,
        ModelSource::ModelScope => Repository {
            siblings: metadata
                .json::<ModelScopeResponse>()
                .await
                .map_err(|e| AppError::new("model.catalog", e.to_string()))?
                .data
                .files
                .into_iter()
                .map(|file| RepositoryFile {
                    rfilename: file.path,
                    size: Some(file.size),
                })
                .collect(),
        },
    };
    let files: Vec<_> = repository
        .siblings
        .into_iter()
        .filter(|file| model.files.contains(&file.rfilename.as_str()))
        .collect();
    if files.len() != model.files.len() {
        return Err(AppError::new("model.catalog", "模型仓库缺少必要的推理文件"));
    }
    let total = files.iter().filter_map(|file| file.size).sum::<u64>();
    let mut completed = 0u64;
    let mut last_progress = 5u8;
    for file in &files {
        let destination = target.join(safe_relative_path(&file.rfilename)?);
        let expected = file.size.unwrap_or(0);
        let base_completed = completed;
        let outcome = download_file(
            &client,
            file_url(model, &file.rfilename)?,
            &destination,
            expected,
            running,
            |current, bytes_per_second| {
                let progress = 5
                    + (base_completed + current)
                        .saturating_mul(94)
                        .checked_div(total.max(1))
                        .unwrap_or(0)
                        .min(94) as u8;
                if progress != last_progress {
                    last_progress = progress;
                    let _ = set_state(
                        db,
                        app,
                        model,
                        &target,
                        "downloading",
                        progress,
                        Some("正在下载模型"),
                        Some(base_completed + current),
                        Some(total),
                        Some(bytes_per_second),
                    );
                } else {
                    emit(
                        app,
                        model.id,
                        "downloading",
                        progress,
                        Some(&target),
                        Some("正在下载模型".into()),
                        Some(base_completed + current),
                        Some(total),
                        Some(bytes_per_second),
                    );
                }
            },
        )
        .await?;
        if matches!(outcome, DownloadOutcome::Paused) {
            set_state(
                db,
                app,
                model,
                &target,
                "paused",
                last_progress,
                Some("下载已暂停"),
                Some(completed + destination.metadata().map(|value| value.len()).unwrap_or(0)),
                Some(total),
                Some(0),
            )?;
            return Ok(());
        }
        completed += expected;
    }
    if !model_ready(model, &target) {
        return Err(AppError::new("model.files", "模型文件下载不完整"));
    }
    set_state(
        db,
        app,
        model,
        &target,
        "done",
        100,
        None,
        Some(total),
        Some(total),
        Some(0),
    )
}

fn set_state(
    db: &Database,
    app: &AppHandle,
    model: &CatalogModel,
    target: &Path,
    status: &str,
    progress: u8,
    message: Option<&str>,
    downloaded_bytes: Option<u64>,
    total_bytes: Option<u64>,
    bytes_per_second: Option<u64>,
) -> Result<(), AppError> {
    db.set_model_state(
        model.id,
        model.engine,
        model.name,
        Some(&target.to_string_lossy()),
        status,
        progress,
    )?;
    emit(
        app,
        model.id,
        status,
        progress,
        Some(target),
        message.map(str::to_owned),
        downloaded_bytes,
        total_bytes,
        bytes_per_second,
    );
    Ok(())
}

async fn ensure_runtime(
    app: &AppHandle,
    model: &CatalogModel,
    model_target: &Path,
    app_data: &Path,
    running: &AtomicBool,
) -> Result<bool, AppError> {
    let _guard = RUNTIME_INSTALL.lock().await;
    let runtime = local_asr::runtime_directory(app_data);
    if !runtime_ready(&runtime)
        && !install_runtime_archive(
            app,
            model,
            model_target,
            &runtime,
            RUNTIME_URL,
            RUNTIME_SIZE,
            RUNTIME_SHA256,
            "正在下载共享推理运行时",
            &["onnxruntime.dll", "sherpa-onnx-c-api.dll"],
            running,
            |current, total| {
                current.saturating_mul(5).checked_div(total.max(1)).unwrap_or(0) as u8
            },
        )
        .await?
    {
        return Ok(false);
    }
    if local_asr::has_nvidia_gpu() && !local_asr::cuda_runtime_installed(app_data) {
        let cuda_runtime = local_asr::cuda_runtime_directory(app_data);
        if let Err(error) = copy_dir_all(&runtime, &cuda_runtime) {
            emit(
                app,
                model.id,
                "downloading",
                5,
                Some(model_target),
                Some(format!("CUDA 运行时准备失败，已回退 CPU：{error}")),
                None,
                None,
                None,
            );
            return Ok(true);
        }
        match install_runtime_archive(
            app,
            model,
            model_target,
            &cuda_runtime,
            CUDA_RUNTIME_URL,
            CUDA_RUNTIME_SIZE,
            CUDA_RUNTIME_SHA256,
            "正在下载 CUDA 推理运行时",
            &[
                "onnxruntime.dll",
                "sherpa-onnx-c-api.dll",
                "onnxruntime_providers_cuda.dll",
            ],
            running,
            |current, total| {
                (current.saturating_mul(5).checked_div(total.max(1)).unwrap_or(0) as u8).min(5)
            },
        )
        .await
        {
            Ok(false) => return Ok(false),
            Ok(true) => {}
            Err(error) => {
                emit(
                    app,
                    model.id,
                    "downloading",
                    5,
                    Some(model_target),
                    Some(format!("CUDA 运行时安装失败，已回退 CPU：{}", error.message)),
                    None,
                    None,
                    None,
                );
                let _ = std::fs::remove_dir_all(&cuda_runtime);
            }
        }
    }
    Ok(true)
}

async fn install_runtime_archive(
    app: &AppHandle,
    model: &CatalogModel,
    model_target: &Path,
    runtime: &Path,
    url: &str,
    size: u64,
    sha256: &str,
    message: &str,
    required_files: &[&str],
    running: &AtomicBool,
    progress_for: impl Fn(u64, u64) -> u8,
) -> Result<bool, AppError> {
    std::fs::create_dir_all(runtime)
        .map_err(|e| AppError::new("runtime.directory", e.to_string()))?;
    let archive = runtime.join("runtime.tar.bz2.part");
    let client = download_client()?;
    let outcome = download_file(
        &client,
        url::Url::parse(url).expect("valid runtime URL"),
        &archive,
        size,
        running,
        |current, bytes_per_second| {
            emit(
                app,
                model.id,
                "downloading",
                progress_for(current, size),
                Some(model_target),
                Some(message.into()),
                Some(current),
                Some(size),
                Some(bytes_per_second),
            );
        },
    )
    .await?;
    if matches!(outcome, DownloadOutcome::Paused) {
        return Ok(false);
    }
    verify_sha256(&archive, sha256)?;
    let archive_for_extract = archive.clone();
    let runtime_for_extract = runtime.to_owned();
    tokio::task::spawn_blocking(move || -> Result<(), AppError> {
        let file = File::open(&archive_for_extract)
            .map_err(|e| AppError::new("runtime.archive", e.to_string()))?;
        tar::Archive::new(BzDecoder::new(file))
            .unpack(&runtime_for_extract)
            .map_err(|e| AppError::new("runtime.extract", e.to_string()))
    })
    .await
    .map_err(|e| AppError::new("runtime.extract", e.to_string()))??;
    if required_files
        .iter()
        .any(|name| find_named_file(runtime, name).is_none())
    {
        return Err(AppError::new("runtime.extract", "推理运行时解压不完整"));
    }
    std::fs::write(runtime.join(".complete"), sha256)
        .map_err(|e| AppError::new("runtime.marker", e.to_string()))?;
    let _ = std::fs::remove_file(archive);
    Ok(true)
}

fn download_client() -> Result<reqwest::Client, AppError> {
    reqwest::Client::builder()
        .user_agent(DOWNLOAD_USER_AGENT)
        .build()
        .map_err(|e| AppError::new("model.client", e.to_string()))
}

async fn download_file<F>(
    client: &reqwest::Client,
    url: url::Url,
    destination: &Path,
    expected_size: u64,
    running: &AtomicBool,
    mut progress: F,
) -> Result<DownloadOutcome, AppError>
where
    F: FnMut(u64, u64),
{
    if !running.load(Ordering::Relaxed) {
        return Ok(DownloadOutcome::Paused);
    }
    if let Some(parent) = destination.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| AppError::new("model.directory", e.to_string()))?;
    }
    let mut existing = destination.metadata().map(|value| value.len()).unwrap_or(0);
    if expected_size > 0 && existing == expected_size {
        progress(existing, 0);
        return Ok(DownloadOutcome::Complete);
    }
    if expected_size > 0 && existing > expected_size {
        existing = 0;
    }
    let mut request = client.get(url);
    if existing > 0 {
        request = request.header(reqwest::header::RANGE, format!("bytes={existing}-"));
    }
    let response = request
        .send()
        .await
        .map_err(|e| AppError::new("model.network", e.to_string()).retryable())?;
    if !response.status().is_success() {
        return Err(AppError::new(
            "model.download",
            format!("下载失败：{}", response.status()),
        ));
    }
    let append = existing > 0 && response.status() == reqwest::StatusCode::PARTIAL_CONTENT;
    let base = if append { existing } else { 0 };
    let started_at = Instant::now();
    let mut last_report = Instant::now();
    progress(base, 0);
    let mut output = OpenOptions::new()
        .create(true)
        .write(true)
        .append(append)
        .truncate(!append)
        .open(destination)
        .map_err(|e| AppError::new("model.file", e.to_string()))?;
    let mut written = 0u64;
    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        if !running.load(Ordering::Relaxed) {
            return Ok(DownloadOutcome::Paused);
        }
        let chunk = chunk.map_err(|e| AppError::new("model.network", e.to_string()).retryable())?;
        output
            .write_all(&chunk)
            .map_err(|e| AppError::new("model.file", e.to_string()))?;
        written += chunk.len() as u64;
        if last_report.elapsed() >= Duration::from_millis(250) {
            let bytes_per_second =
                (written as f64 / started_at.elapsed().as_secs_f64().max(0.001)) as u64;
            progress(base + written, bytes_per_second);
            last_report = Instant::now();
        }
    }
    let actual = base + written;
    let bytes_per_second = (written as f64 / started_at.elapsed().as_secs_f64().max(0.001)) as u64;
    progress(actual, bytes_per_second);
    if expected_size > 0 && actual != expected_size {
        return Err(AppError::new(
            "model.size",
            format!("下载文件大小不匹配：需要 {expected_size} 字节，实际 {actual} 字节"),
        ));
    }
    Ok(DownloadOutcome::Complete)
}

fn verify_sha256(path: &Path, expected: &str) -> Result<(), AppError> {
    let mut file = File::open(path).map_err(|e| AppError::new("runtime.hash", e.to_string()))?;
    let mut hash = Sha256::new();
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let read = file
            .read(&mut buffer)
            .map_err(|e| AppError::new("runtime.hash", e.to_string()))?;
        if read == 0 {
            break;
        }
        hash.update(&buffer[..read]);
    }
    if format!("{:x}", hash.finalize()) == expected {
        Ok(())
    } else {
        let _ = std::fs::remove_file(path);
        Err(AppError::new(
            "runtime.hash",
            "推理运行时校验失败，请重新下载",
        ))
    }
}

fn find_named_file(root: &Path, name: &str) -> Option<PathBuf> {
    for entry in std::fs::read_dir(root).ok()?.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if let Some(found) = find_named_file(&path, name) {
                return Some(found);
            }
        } else if path.file_name().is_some_and(|value| value == name) {
            return Some(path);
        }
    }
    None
}

fn runtime_ready(root: &Path) -> bool {
    root.join(".complete").is_file()
        && find_named_file(root, "onnxruntime.dll").is_some()
        && find_named_file(root, "sherpa-onnx-c-api.dll").is_some()
}

fn copy_dir_all(from: &Path, to: &Path) -> Result<(), std::io::Error> {
    std::fs::create_dir_all(to)?;
    for entry in std::fs::read_dir(from)? {
        let entry = entry?;
        let source = entry.path();
        let destination = to.join(entry.file_name());
        if source.is_dir() {
            copy_dir_all(&source, &destination)?;
        } else if entry.file_name() != ".complete" {
            std::fs::copy(&source, &destination)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_downloads_only_required_int8_assets() {
        for model in catalog() {
            assert!(model.files.iter().any(|name| name.ends_with(".int8.onnx")));
            if model.engine == "qwen3-asr" {
                assert!(model.files.contains(&"model_1.7B/conv_frontend.onnx"));
                assert!(model.files.contains(&"tokenizer/vocab.json"));
            } else {
                assert!(model.files.iter().any(|name| name.ends_with("tokens.txt")));
            }
        }
    }

    #[test]
    fn runtime_release_is_pinned_and_verified() {
        assert!(RUNTIME_URL.contains("/v1.13.7/"));
        assert_eq!(RUNTIME_SHA256.len(), 64);
        assert!(CUDA_RUNTIME_URL.contains("/v1.13.7/"));
        assert!(CUDA_RUNTIME_URL.contains("cuda-12.x"));
        assert_eq!(CUDA_RUNTIME_SHA256.len(), 64);
        assert_eq!(CUDA_RUNTIME_SIZE, 375_601_142);
    }

    #[test]
    fn copies_cpu_runtime_before_overlaying_cuda_files() {
        let temp = std::env::temp_dir().join(format!("yuliu-runtime-copy-{}", std::process::id()));
        let from = temp.join("cpu");
        let to = temp.join("cuda");
        std::fs::create_dir_all(from.join("bin")).unwrap();
        std::fs::write(from.join("bin/sherpa-onnx-c-api.dll"), b"cpu").unwrap();
        std::fs::write(from.join(".complete"), b"cpu-hash").unwrap();
        copy_dir_all(&from, &to).unwrap();
        assert!(to.join("bin/sherpa-onnx-c-api.dll").is_file());
        assert!(!to.join(".complete").is_file());
        let _ = std::fs::remove_dir_all(temp);
    }

    #[test]
    fn modelscope_download_url_keeps_the_nested_model_path() {
        let model = find("qwen3-asr-1.7b").expect("Qwen3-ASR is in the catalog");
        let url = file_url(&model, "model_1.7B/decoder.int8.onnx").unwrap();
        assert_eq!(url.domain(), Some("www.modelscope.cn"));
        assert!(url
            .query_pairs()
            .any(|(key, value)| key == "FilePath" && value == "model_1.7B/decoder.int8.onnx"));
    }
}
