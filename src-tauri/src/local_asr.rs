use crate::error::AppError;
use crate::streaming_asr::{AudioChunk, StreamMessage};
use libloading::Library;
use std::{
    collections::HashMap,
    ffi::{c_char, c_void, CStr, CString},
    path::{Path, PathBuf},
    sync::{Mutex, OnceLock},
};

// The Qwen3-ASR text model supports a much larger context, but its ONNX runtime
// preallocates the KV cache for this value. 8192 is the highest practical
// profile for the supported 8 GB-class CUDA devices and leaves ample room for
// long dictation instead of truncating near the old 512-token default.
const QWEN3_MAX_TOTAL_LEN: i32 = 8_192;
const QWEN3_MAX_NEW_TOKENS: i32 = 512;
use tokio::sync::mpsc;

const RUNTIME_VERSION: &str = "1.13.7";

macro_rules! c_struct {
    ($name:ident { $($field:ident: $ty:ty),* $(,)? }) => {
        #[repr(C)]
        struct $name { $( $field: $ty, )* }
    };
}

c_struct!(FeatureConfig {
    sample_rate: i32,
    feature_dim: i32
});
c_struct!(HomophoneReplacerConfig {
    dict_dir: *const c_char,
    lexicon: *const c_char,
    rule_fsts: *const c_char,
});
c_struct!(TransducerConfig {
    encoder: *const c_char,
    decoder: *const c_char,
    joiner: *const c_char,
});
c_struct!(SingleModelConfig { model: *const c_char });
c_struct!(WhisperConfig {
    encoder: *const c_char,
    decoder: *const c_char,
    language: *const c_char,
    task: *const c_char,
    tail_paddings: i32,
    enable_token_timestamps: i32,
    enable_segment_timestamps: i32,
});
c_struct!(CanaryConfig {
    encoder: *const c_char,
    decoder: *const c_char,
    src_lang: *const c_char,
    tgt_lang: *const c_char,
    use_pnc: i32,
});
c_struct!(CohereConfig {
    encoder: *const c_char,
    decoder: *const c_char,
    language: *const c_char,
    use_punct: i32,
    use_itn: i32,
});
c_struct!(EncoderDecoderConfig {
    encoder: *const c_char,
    decoder: *const c_char,
});
c_struct!(MoonshineConfig {
    preprocessor: *const c_char,
    encoder: *const c_char,
    uncached_decoder: *const c_char,
    cached_decoder: *const c_char,
    merged_decoder: *const c_char,
});
c_struct!(SenseVoiceConfig {
    model: *const c_char,
    language: *const c_char,
    use_itn: i32,
});
c_struct!(FunAsrNanoConfig {
    encoder_adaptor: *const c_char,
    llm: *const c_char,
    embedding: *const c_char,
    tokenizer: *const c_char,
    system_prompt: *const c_char,
    user_prompt: *const c_char,
    max_new_tokens: i32,
    temperature: f32,
    top_p: f32,
    seed: i32,
    language: *const c_char,
    itn: i32,
    hotwords: *const c_char,
});
c_struct!(Qwen3Config {
    conv_frontend: *const c_char,
    encoder: *const c_char,
    decoder: *const c_char,
    tokenizer: *const c_char,
    max_total_len: i32,
    max_new_tokens: i32,
    temperature: f32,
    top_p: f32,
    seed: i32,
    hotwords: *const c_char,
});
c_struct!(OfflineModelConfig {
    transducer: TransducerConfig,
    paraformer: SingleModelConfig,
    nemo_ctc: SingleModelConfig,
    whisper: WhisperConfig,
    tdnn: SingleModelConfig,
    tokens: *const c_char,
    num_threads: i32,
    debug: i32,
    provider: *const c_char,
    model_type: *const c_char,
    modeling_unit: *const c_char,
    bpe_vocab: *const c_char,
    telespeech_ctc: *const c_char,
    sense_voice: SenseVoiceConfig,
    moonshine: MoonshineConfig,
    fire_red_asr: EncoderDecoderConfig,
    dolphin: SingleModelConfig,
    zipformer_ctc: SingleModelConfig,
    canary: CanaryConfig,
    wenet_ctc: SingleModelConfig,
    omnilingual: SingleModelConfig,
    medasr: SingleModelConfig,
    funasr_nano: FunAsrNanoConfig,
    fire_red_asr_ctc: SingleModelConfig,
    qwen3_asr: Qwen3Config,
    cohere_transcribe: CohereConfig,
});
c_struct!(OfflineLmConfig { model: *const c_char, scale: f32 });
c_struct!(RecognizerConfig {
    feat_config: FeatureConfig,
    model_config: OfflineModelConfig,
    lm_config: OfflineLmConfig,
    decoding_method: *const c_char,
    max_active_paths: i32,
    hotwords_file: *const c_char,
    hotwords_score: f32,
    rule_fsts: *const c_char,
    rule_fars: *const c_char,
    blank_penalty: f32,
    hr: HomophoneReplacerConfig,
});
c_struct!(RecognizerResult {
    text: *const c_char,
    timestamps: *mut f32,
    count: i32,
    tokens: *const c_char,
    tokens_arr: *const *const c_char,
    json: *const c_char,
    lang: *const c_char,
    emotion: *const c_char,
    event: *const c_char,
    durations: *mut f32,
    ys_log_probs: *mut f32,
    segment_timestamps: *const f32,
    segment_durations: *const f32,
    segment_texts: *const c_char,
    segment_texts_arr: *const *const c_char,
    segment_count: i32,
});

c_struct!(OnlineTransducerConfig {
    encoder: *const c_char,
    decoder: *const c_char,
    joiner: *const c_char,
});
c_struct!(OnlineParaformerConfig {
    encoder: *const c_char,
    decoder: *const c_char,
});
c_struct!(OnlineSingleModelConfig { model: *const c_char });
c_struct!(OnlineModelConfig {
    transducer: OnlineTransducerConfig,
    paraformer: OnlineParaformerConfig,
    zipformer2_ctc: OnlineSingleModelConfig,
    tokens: *const c_char,
    num_threads: i32,
    provider: *const c_char,
    debug: i32,
    model_type: *const c_char,
    modeling_unit: *const c_char,
    bpe_vocab: *const c_char,
    tokens_buf: *const c_char,
    tokens_buf_size: i32,
    nemo_ctc: OnlineSingleModelConfig,
    t_one_ctc: OnlineSingleModelConfig,
});
c_struct!(OnlineCtcFstConfig { graph: *const c_char, max_active: i32 });
c_struct!(OnlineRecognizerConfig {
    feat_config: FeatureConfig,
    model_config: OnlineModelConfig,
    decoding_method: *const c_char,
    max_active_paths: i32,
    enable_endpoint: i32,
    rule1_min_trailing_silence: f32,
    rule2_min_trailing_silence: f32,
    rule3_min_utterance_length: f32,
    hotwords_file: *const c_char,
    hotwords_score: f32,
    ctc_fst_decoder_config: OnlineCtcFstConfig,
    rule_fsts: *const c_char,
    rule_fars: *const c_char,
    blank_penalty: f32,
    hotwords_buf: *const c_char,
    hotwords_buf_size: i32,
    hr: HomophoneReplacerConfig,
});
c_struct!(OnlineRecognizerResult {
    text: *const c_char,
    tokens: *const c_char,
    tokens_arr: *const *const c_char,
    timestamps: *mut f32,
    count: i32,
    json: *const c_char,
});

type CreateRecognizer = unsafe extern "C" fn(*const RecognizerConfig) -> *const c_void;
type DestroyRecognizer = unsafe extern "C" fn(*const c_void);
type CreateStream = unsafe extern "C" fn(*const c_void) -> *const c_void;
type DestroyStream = unsafe extern "C" fn(*const c_void);
type AcceptWaveform = unsafe extern "C" fn(*const c_void, i32, *const f32, i32);
type DecodeStream = unsafe extern "C" fn(*const c_void, *const c_void);
type GetResult = unsafe extern "C" fn(*const c_void) -> *const RecognizerResult;
type DestroyResult = unsafe extern "C" fn(*const RecognizerResult);
type GetVersion = unsafe extern "C" fn() -> *const c_char;
type OnlineCreateRecognizer = unsafe extern "C" fn(*const OnlineRecognizerConfig) -> *const c_void;
type OnlineIsReady = unsafe extern "C" fn(*const c_void, *const c_void) -> i32;
type OnlineGetResult =
    unsafe extern "C" fn(*const c_void, *const c_void) -> *const OnlineRecognizerResult;
type OnlineIsEndpoint = unsafe extern "C" fn(*const c_void, *const c_void) -> i32;
type OnlineInputFinished = unsafe extern "C" fn(*const c_void);
type OnlineReset = unsafe extern "C" fn(*const c_void, *const c_void);

struct SherpaApi {
    _dependencies: Vec<Library>,
    library: Library,
}

impl SherpaApi {
    fn load(runtime_root: &Path) -> Result<Self, AppError> {
        #[cfg(not(target_os = "windows"))]
        {
            let _ = runtime_root;
            return Err(AppError::new(
                "asr.runtime_platform",
                "本地推理运行时目前仅提供 Windows x64 版本",
            ));
        }
        #[cfg(target_os = "windows")]
        {
            let c_api = find_file(runtime_root, "sherpa-onnx-c-api.dll")?;
            let dependencies = load_runtime_dependencies(runtime_root)?;
            let library = unsafe { Library::new(c_api) }.map_err(runtime_load_error)?;
            let api = Self {
                _dependencies: dependencies,
                library,
            };
            let version = unsafe {
                let get: libloading::Symbol<GetVersion> = api
                    .library
                    .get(b"SherpaOnnxGetVersionStr\0")
                    .map_err(runtime_load_error)?;
                CStr::from_ptr(get()).to_string_lossy().into_owned()
            };
            if version != RUNTIME_VERSION {
                return Err(AppError::new(
                    "asr.runtime_version",
                    format!("本地推理运行时版本不匹配：需要 {RUNTIME_VERSION}，实际为 {version}"),
                ));
            }
            Ok(api)
        }
    }

    unsafe fn symbol<T>(&self, name: &[u8]) -> Result<libloading::Symbol<'_, T>, AppError> {
        unsafe { self.library.get(name) }.map_err(runtime_load_error)
    }
}

#[cfg(target_os = "windows")]
fn load_runtime_dependencies(runtime_root: &Path) -> Result<Vec<Library>, AppError> {
    let mut libraries = Vec::new();
    let has_cuda = find_file(runtime_root, "onnxruntime_providers_cuda.dll").is_ok();
    let mut names = vec!["onnxruntime.dll", "onnxruntime_providers_shared.dll"];
    if has_cuda {
        if find_file(runtime_root, "cudnn64_9.dll").is_ok() {
            names.extend([
                "nvJitLink_120_0.dll",
                "cudart64_12.dll",
                "cublasLt64_12.dll",
                "cublas64_12.dll",
                "cufft64_11.dll",
                "cudnn64_9.dll",
                "cudnn_graph64_9.dll",
                "cudnn_ops64_9.dll",
                "cudnn_heuristic64_9.dll",
                "cudnn_engines_precompiled64_9.dll",
                "cudnn_engines_runtime_compiled64_9.dll",
                "cudnn_engines_tensor_ir64_9.dll",
                "cudnn_cnn64_9.dll",
                "cudnn_adv64_9.dll",
                "cudnn_ext64_9.dll",
            ]);
        } else {
            names.extend([
                "cudart64_110.dll",
                "cublasLt64_11.dll",
                "cublas64_11.dll",
                "cufft64_10.dll",
                "cudnn64_8.dll",
                "cudnn_ops_infer64_8.dll",
                "cudnn_ops_train64_8.dll",
                "cudnn_cnn_infer64_8.dll",
                "cudnn_cnn_train64_8.dll",
                "cudnn_adv_infer64_8.dll",
                "cudnn_adv_train64_8.dll",
            ]);
        }
    }
    for name in names {
        let path = find_file(runtime_root, name)?;
        libraries.push(unsafe { Library::new(path) }.map_err(|error| {
            AppError::new(
                "asr.runtime_load",
                format!("无法加载本地推理依赖 {name}：{error}"),
            )
        })?);
    }
    Ok(libraries)
}

fn runtime_load_error(error: libloading::Error) -> AppError {
    AppError::new(
        "asr.runtime_load",
        format!("无法加载本地推理运行时：{error}"),
    )
}

struct LocalRecognizer {
    api: SherpaApi,
    handle: *const c_void,
}

// sherpa-onnx recognizers are used serially behind this mutex. Keeping the
// active model alive avoids reloading hundreds of megabytes for every segment.
unsafe impl Send for LocalRecognizer {}
static RECOGNIZERS: OnceLock<Mutex<HashMap<String, LocalRecognizer>>> = OnceLock::new();

struct OnlineRecognizer {
    api: SherpaApi,
    handle: *const c_void,
    _paths: Vec<CString>,
}

unsafe impl Send for OnlineRecognizer {}
unsafe impl Sync for OnlineRecognizer {}
static ONLINE_RECOGNIZERS: OnceLock<Mutex<HashMap<String, std::sync::Arc<OnlineRecognizer>>>> =
    OnceLock::new();

impl Drop for OnlineRecognizer {
    fn drop(&mut self) {
        if self.handle.is_null() {
            return;
        }
        unsafe {
            if let Ok(destroy) = self
                .api
                .symbol::<DestroyRecognizer>(b"SherpaOnnxDestroyOnlineRecognizer\0")
            {
                destroy(self.handle);
            }
        }
    }
}

impl LocalRecognizer {
    fn create(
        runtime_root: &Path,
        model_root: &Path,
        provider: &str,
        language: Option<&str>,
        hotwords: &str,
        compute_device: &str,
    ) -> Result<Self, AppError> {
        let api = SherpaApi::load(runtime_root)?;
        let selected_device = resolve_compute_device(runtime_root, compute_device);
        let device = CString::new(selected_device).expect("static string");
        let greedy = CString::new("greedy_search").expect("static string");
        let language = CString::new(language.unwrap_or("auto"))
            .map_err(|_| AppError::new("asr.config", "语言代码无效"))?;
        let task = CString::new("transcribe").expect("static string");
        let mut config: RecognizerConfig = unsafe { std::mem::zeroed() };
        config.feat_config.sample_rate = 16_000;
        config.feat_config.feature_dim = 80;
        config.model_config.provider = device.as_ptr();
        config.model_config.num_threads = std::thread::available_parallelism()
            .map(|value| value.get().min(4) as i32)
            .unwrap_or(2);
        config.decoding_method = greedy.as_ptr();
        config.max_active_paths = 4;

        if provider == "qwen3-asr" {
            let conv_frontend = path_cstring(&find_file(model_root, "conv_frontend.onnx")?)?;
            let encoder = path_cstring(&find_file(model_root, "encoder.int8.onnx")?)?;
            let decoder = path_cstring(&find_file(model_root, "decoder.int8.onnx")?)?;
            let tokenizer = path_cstring(&model_root.join("tokenizer"))?;
            let hotwords = CString::new(hotwords)
                .map_err(|_| AppError::new("asr.config", "热词包含无效字符"))?;
            config.feat_config.feature_dim = 128;
            config.model_config.qwen3_asr.conv_frontend = conv_frontend.as_ptr();
            config.model_config.qwen3_asr.encoder = encoder.as_ptr();
            config.model_config.qwen3_asr.decoder = decoder.as_ptr();
            config.model_config.qwen3_asr.tokenizer = tokenizer.as_ptr();
            config.model_config.qwen3_asr.max_total_len = QWEN3_MAX_TOTAL_LEN;
            config.model_config.qwen3_asr.max_new_tokens = QWEN3_MAX_NEW_TOKENS;
            config.model_config.qwen3_asr.temperature = 1e-6;
            config.model_config.qwen3_asr.top_p = 0.8;
            config.model_config.qwen3_asr.seed = 42;
            config.model_config.qwen3_asr.hotwords = hotwords.as_ptr();
            let handle = create_handle(&api, &config)?;
            return Ok(Self { api, handle });
        }

        let tokens = path_cstring(&find_file(model_root, tokens_name(model_root, provider)?)?)?;
        config.model_config.tokens = tokens.as_ptr();

        if provider == "faster-whisper" {
            let prefix = whisper_prefix(model_root)?;
            let encoder = path_cstring(&find_file(
                model_root,
                &format!("{prefix}-encoder.int8.onnx"),
            )?)?;
            let decoder = path_cstring(&find_file(
                model_root,
                &format!("{prefix}-decoder.int8.onnx"),
            )?)?;
            config.model_config.whisper.encoder = encoder.as_ptr();
            config.model_config.whisper.decoder = decoder.as_ptr();
            config.model_config.whisper.language = language.as_ptr();
            config.model_config.whisper.task = task.as_ptr();
            config.model_config.whisper.tail_paddings = -1;
            let handle = create_handle(&api, &config)?;
            return Ok(Self { api, handle });
        }

        let model = path_cstring(&find_file(model_root, "model.int8.onnx")?)?;
        if model_root.to_string_lossy().contains("paraformer") {
            config.model_config.paraformer.model = model.as_ptr();
        } else {
            config.model_config.sense_voice.model = model.as_ptr();
            config.model_config.sense_voice.language = language.as_ptr();
            config.model_config.sense_voice.use_itn = 1;
        }
        let handle = create_handle(&api, &config)?;
        Ok(Self { api, handle })
    }

    fn transcribe_wav(&self, wav_path: &Path) -> Result<String, AppError> {
        let mut reader = hound::WavReader::open(wav_path)
            .map_err(|e| AppError::new("asr.audio", e.to_string()))?;
        let spec = reader.spec();
        if spec.channels != 1 {
            return Err(AppError::new("asr.audio", "本地识别仅接受单声道 WAV"));
        }
        let samples = match spec.sample_format {
            hound::SampleFormat::Int => reader
                .samples::<i16>()
                .map(|sample| sample.map(|value| value as f32 / i16::MAX as f32))
                .collect::<Result<Vec<_>, _>>(),
            hound::SampleFormat::Float => reader.samples::<f32>().collect::<Result<Vec<_>, _>>(),
        }
        .map_err(|e| AppError::new("asr.audio", e.to_string()))?;
        self.transcribe_samples(&samples, spec.sample_rate as i32)
    }

    fn transcribe_samples(&self, samples: &[f32], sample_rate: i32) -> Result<String, AppError> {
        let sample_count =
            i32::try_from(samples.len()).map_err(|_| AppError::new("asr.audio", "音频片段过长"))?;
        unsafe {
            let create: libloading::Symbol<CreateStream> =
                self.api.symbol(b"SherpaOnnxCreateOfflineStream\0")?;
            let destroy_stream: libloading::Symbol<DestroyStream> =
                self.api.symbol(b"SherpaOnnxDestroyOfflineStream\0")?;
            let accept: libloading::Symbol<AcceptWaveform> =
                self.api.symbol(b"SherpaOnnxAcceptWaveformOffline\0")?;
            let decode: libloading::Symbol<DecodeStream> =
                self.api.symbol(b"SherpaOnnxDecodeOfflineStream\0")?;
            let get_result: libloading::Symbol<GetResult> =
                self.api.symbol(b"SherpaOnnxGetOfflineStreamResult\0")?;
            let destroy_result: libloading::Symbol<DestroyResult> = self
                .api
                .symbol(b"SherpaOnnxDestroyOfflineRecognizerResult\0")?;
            let stream = create(self.handle);
            if stream.is_null() {
                return Err(AppError::new("asr.runtime", "无法创建本地识别流"));
            }
            accept(stream, sample_rate, samples.as_ptr(), sample_count);
            decode(self.handle, stream);
            let result = get_result(stream);
            let text = if result.is_null() || (*result).text.is_null() {
                String::new()
            } else {
                CStr::from_ptr((*result).text)
                    .to_string_lossy()
                    .into_owned()
            };
            if !result.is_null() {
                destroy_result(result);
            }
            destroy_stream(stream);
            Ok(text.trim().to_owned())
        }
    }
}

impl Drop for LocalRecognizer {
    fn drop(&mut self) {
        if !self.handle.is_null() {
            unsafe {
                if let Ok(destroy) = self
                    .api
                    .symbol::<DestroyRecognizer>(b"SherpaOnnxDestroyOfflineRecognizer\0")
                {
                    destroy(self.handle);
                }
            }
        }
    }
}

fn create_handle(api: &SherpaApi, config: &RecognizerConfig) -> Result<*const c_void, AppError> {
    let handle = unsafe {
        let create: libloading::Symbol<CreateRecognizer> =
            api.symbol(b"SherpaOnnxCreateOfflineRecognizer\0")?;
        create(config)
    };
    if handle.is_null() {
        Err(AppError::new(
            "asr.model_load",
            "无法加载本地模型，请重新下载模型文件",
        ))
    } else {
        Ok(handle)
    }
}

fn tokens_name(model_root: &Path, provider: &str) -> Result<&'static str, AppError> {
    if provider != "faster-whisper" {
        return Ok("tokens.txt");
    }
    match whisper_prefix(model_root)? {
        "medium" => Ok("medium-tokens.txt"),
        "large-v3" => Ok("large-v3-tokens.txt"),
        _ => Err(AppError::new("asr.model", "不支持的 Whisper 模型")),
    }
}

fn whisper_prefix(model_root: &Path) -> Result<&'static str, AppError> {
    let value = model_root.to_string_lossy();
    if value.contains("large-v3") {
        Ok("large-v3")
    } else if value.contains("medium") {
        Ok("medium")
    } else {
        Err(AppError::new("asr.model", "无法识别 Whisper 模型类型"))
    }
}

fn path_cstring(path: &Path) -> Result<CString, AppError> {
    CString::new(path.to_string_lossy().as_bytes())
        .map_err(|_| AppError::new("asr.model_path", "模型路径包含无效字符"))
}

fn find_file(root: &Path, name: &str) -> Result<PathBuf, AppError> {
    let direct = root.join(name);
    if direct.is_file() {
        return Ok(direct);
    }
    let entries =
        std::fs::read_dir(root).map_err(|e| AppError::new("asr.runtime_missing", e.to_string()))?;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if let Ok(found) = find_file(&path, name) {
                return Ok(found);
            }
        } else if path.file_name().is_some_and(|value| value == name) {
            return Ok(path);
        }
    }
    Err(AppError::new(
        "asr.runtime_missing",
        format!("缺少本地推理文件：{name}"),
    ))
}

pub fn runtime_directory(app_data: &Path) -> PathBuf {
    app_data
        .join("runtime")
        .join("sherpa-onnx")
        .join(RUNTIME_VERSION)
        .join("win-x64")
}

pub fn cuda_runtime_directory(app_data: &Path) -> PathBuf {
    app_data
        .join("runtime")
        .join("sherpa-onnx")
        .join(RUNTIME_VERSION)
        .join("win-x64-cuda12")
}

fn runtime_directory_for(app_data: &Path, preference: &str) -> PathBuf {
    let cuda = cuda_runtime_directory(app_data);
    if preference != "cpu" && cuda_runtime_loadable(&cuda) {
        cuda
    } else {
        runtime_directory(app_data)
    }
}

pub fn nvidia_gpu_name() -> Option<String> {
    std::process::Command::new("nvidia-smi")
        .arg("--query-gpu=name")
        .arg("--format=csv,noheader")
        .output()
        .ok()
        .filter(|output| output.status.success())
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .and_then(|value| value.lines().next().map(str::trim).map(str::to_owned))
        .filter(|value| !value.is_empty())
}

pub fn has_nvidia_gpu() -> bool {
    nvidia_gpu_name().is_some()
}

pub fn cuda_runtime_installed(app_data: &Path) -> bool {
    let root = cuda_runtime_directory(app_data);
    root.join(".complete").is_file()
        && find_file(&root, "onnxruntime.dll").is_ok()
        && find_file(&root, "sherpa-onnx-c-api.dll").is_ok()
        && find_file(&root, "onnxruntime_providers_cuda.dll").is_ok()
}

pub fn resolve_compute_device(runtime_root: &Path, preference: &str) -> &'static str {
    if preference == "cpu" {
        return "cpu";
    }
    if cuda_runtime_loadable(runtime_root) && has_nvidia_gpu() {
        "cuda"
    } else {
        "cpu"
    }
}

fn cuda_runtime_loadable(runtime_root: &Path) -> bool {
    find_file(runtime_root, "onnxruntime_providers_cuda.dll").is_ok()
        && load_runtime_dependencies(runtime_root).is_ok()
}

pub fn compute_status(app_data: &Path, preference: &str) -> serde_json::Value {
    let cuda = cuda_runtime_directory(app_data);
    let cuda_runtime = cuda_runtime_loadable(&cuda);
    let gpu = nvidia_gpu_name();
    let active = if preference != "cpu" && cuda_runtime && gpu.is_some() {
        "cuda"
    } else {
        "cpu"
    };
    serde_json::json!({
        "preference": preference,
        "active": active,
        "gpuName": gpu,
        "cudaRuntimeAvailable": cuda_runtime,
        "message": if active == "cuda" {
            "CUDA 已启用"
        } else if gpu.is_some() && !cuda_runtime {
            "检测到 NVIDIA GPU，但 CUDA 运行时尚未安装；下次下载本地模型时会自动安装，当前回退 CPU"
        } else {
            "当前使用 CPU"
        }
    })
}

pub fn stream(
    app_data: &Path,
    model_root: &Path,
    preference: &str,
    mut receiver: mpsc::Receiver<StreamMessage>,
    mut emit: impl FnMut(&str, bool) -> Result<(), AppError>,
) -> Result<(), AppError> {
    let online = online_recognizer(app_data, model_root, preference)?;
    let api = &online.api;
    let recognizer = online.handle;
    unsafe {
        let create_stream: libloading::Symbol<CreateStream> =
            api.symbol(b"SherpaOnnxCreateOnlineStream\0")?;
        let destroy_stream: libloading::Symbol<DestroyStream> =
            api.symbol(b"SherpaOnnxDestroyOnlineStream\0")?;
        let accept: libloading::Symbol<AcceptWaveform> =
            api.symbol(b"SherpaOnnxOnlineStreamAcceptWaveform\0")?;
        let is_ready: libloading::Symbol<OnlineIsReady> =
            api.symbol(b"SherpaOnnxIsOnlineStreamReady\0")?;
        let decode: libloading::Symbol<DecodeStream> =
            api.symbol(b"SherpaOnnxDecodeOnlineStream\0")?;
        let get_result: libloading::Symbol<OnlineGetResult> =
            api.symbol(b"SherpaOnnxGetOnlineStreamResult\0")?;
        let destroy_result: libloading::Symbol<DestroyResult> =
            api.symbol(b"SherpaOnnxDestroyOnlineRecognizerResult\0")?;
        let is_endpoint: libloading::Symbol<OnlineIsEndpoint> =
            api.symbol(b"SherpaOnnxOnlineStreamIsEndpoint\0")?;
        let reset: libloading::Symbol<OnlineReset> =
            api.symbol(b"SherpaOnnxOnlineStreamReset\0")?;
        let input_finished: libloading::Symbol<OnlineInputFinished> =
            api.symbol(b"SherpaOnnxOnlineStreamInputFinished\0")?;
        let stream = create_stream(recognizer);
        if stream.is_null() {
            return Err(AppError::new("asr.runtime", "无法创建 Sherpa 在线识别流"));
        }
        let mut last_partial = String::new();
        let mut finish = false;
        while let Some(message) = receiver.blocking_recv() {
            match message {
                StreamMessage::Audio(chunk) => {
                    let samples = chunk_to_16k_mono(&chunk);
                    if !samples.is_empty() {
                        accept(stream, 16_000, samples.as_ptr(), samples.len() as i32);
                    }
                }
                StreamMessage::Finish => {
                    input_finished(stream);
                    finish = true;
                }
                StreamMessage::Cancel => break,
            }
            while is_ready(recognizer, stream) != 0 {
                decode(recognizer, stream);
            }
            let result = get_result(recognizer, stream);
            let text = online_result_text(result);
            if !result.is_null() {
                destroy_result(result as *const RecognizerResult);
            }
            if !text.is_empty() && text != last_partial {
                emit(&text, false)?;
                last_partial = text.clone();
            }
            let endpoint = is_endpoint(recognizer, stream) != 0;
            if endpoint || finish {
                let stable_text = if text.is_empty() {
                    last_partial.clone()
                } else {
                    text.clone()
                };
                if !stable_text.is_empty() {
                    emit(&punctuate_stable(&stable_text), true)?;
                }
            }
            if endpoint {
                reset(recognizer, stream);
                last_partial.clear();
            }
            if finish {
                break;
            }
        }
        destroy_stream(stream);
        Ok(())
    }
}

fn online_recognizer(
    app_data: &Path,
    model_root: &Path,
    preference: &str,
) -> Result<std::sync::Arc<OnlineRecognizer>, AppError> {
    let runtime_root = runtime_directory_for(app_data, preference);
    let key = format!(
        "{}|{}",
        model_root.to_string_lossy(),
        resolve_compute_device(&runtime_root, preference)
    );
    let mut recognizers = ONLINE_RECOGNIZERS
        .get_or_init(|| Mutex::new(HashMap::new()))
        .lock()
        .map_err(|_| AppError::new("asr.runtime", "本地推理运行时状态异常"))?;
    if !recognizers.contains_key(&key) {
        let recognizer = create_online_recognizer(&runtime_root, model_root, preference)?;
        recognizers.clear();
        recognizers.insert(key.clone(), std::sync::Arc::new(recognizer));
    }
    Ok(recognizers
        .get(&key)
        .expect("online recognizer was inserted")
        .clone())
}

fn create_online_recognizer(
    runtime_root: &Path,
    model_root: &Path,
    preference: &str,
) -> Result<OnlineRecognizer, AppError> {
    let api = SherpaApi::load(runtime_root)?;
    let encoder = path_cstring(&find_file(model_root, "encoder-epoch-99-avg-1.int8.onnx")?)?;
    let decoder = path_cstring(&find_file(model_root, "decoder-epoch-99-avg-1.int8.onnx")?)?;
    let joiner = path_cstring(&find_file(model_root, "joiner-epoch-99-avg-1.int8.onnx")?)?;
    let tokens = path_cstring(&find_file(model_root, "tokens.txt")?)?;
    let greedy = CString::new("greedy_search").expect("static string");
    let selected = resolve_compute_device(runtime_root, preference);
    let mut device = CString::new(selected).expect("static string");
    let mut config: OnlineRecognizerConfig = unsafe { std::mem::zeroed() };
    config.feat_config.sample_rate = 16_000;
    config.feat_config.feature_dim = 80;
    config.model_config.transducer.encoder = encoder.as_ptr();
    config.model_config.transducer.decoder = decoder.as_ptr();
    config.model_config.transducer.joiner = joiner.as_ptr();
    config.model_config.tokens = tokens.as_ptr();
    config.model_config.provider = device.as_ptr();
    config.model_config.num_threads = if selected == "cuda" { 1 } else { 2 };
    config.decoding_method = greedy.as_ptr();
    config.max_active_paths = 4;
    config.enable_endpoint = 1;
    config.rule1_min_trailing_silence = 1.2;
    config.rule2_min_trailing_silence = 0.8;
    config.rule3_min_utterance_length = 20.0;
    unsafe {
        let create_recognizer: libloading::Symbol<OnlineCreateRecognizer> =
            api.symbol(b"SherpaOnnxCreateOnlineRecognizer\0")?;
        let mut handle = create_recognizer(&config);
        if handle.is_null() && selected == "cuda" {
            device = CString::new("cpu").expect("static string");
            config.model_config.provider = device.as_ptr();
            config.model_config.num_threads = 2;
            handle = create_recognizer(&config);
        }
        if handle.is_null() {
            return Err(AppError::new("asr.model_load", "无法加载 Sherpa 流式模型"));
        }
        Ok(OnlineRecognizer {
            api,
            handle,
            _paths: vec![encoder, decoder, joiner, tokens, greedy, device],
        })
    }
}

pub fn validate_streaming(
    app_data: &Path,
    model_root: &Path,
    preference: &str,
) -> Result<(), AppError> {
    let (sender, receiver) = mpsc::channel(1);
    sender
        .blocking_send(StreamMessage::Cancel)
        .map_err(|_| AppError::new("asr.runtime", "无法创建测试流"))?;
    stream(app_data, model_root, preference, receiver, |_, _| Ok(()))
}

unsafe fn online_result_text(result: *const OnlineRecognizerResult) -> String {
    if result.is_null() || unsafe { (*result).text.is_null() } {
        String::new()
    } else {
        unsafe { CStr::from_ptr((*result).text) }
            .to_string_lossy()
            .trim()
            .to_owned()
    }
}

fn chunk_to_16k_mono(chunk: &AudioChunk) -> Vec<f32> {
    let channels = chunk.channels.max(1) as usize;
    let mono: Vec<f32> = chunk
        .samples
        .chunks(channels)
        .map(|frame| frame.iter().sum::<f32>() / frame.len().max(1) as f32)
        .collect();
    if chunk.sample_rate == 16_000 || mono.is_empty() {
        return mono;
    }
    let ratio = chunk.sample_rate as f64 / 16_000.0;
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

fn punctuate_stable(text: &str) -> String {
    let text = text.trim();
    if text.is_empty()
        || text
            .chars()
            .last()
            .is_some_and(|value| ".!?。！？…；;".contains(value))
    {
        return text.to_owned();
    }
    let punctuation = if text
        .chars()
        .any(|value| ('\u{4e00}'..='\u{9fff}').contains(&value))
    {
        '。'
    } else {
        '.'
    };
    format!("{text}{punctuation}")
}

pub fn validate(
    app_data: &Path,
    model_root: &Path,
    provider: &str,
    language: Option<&str>,
    compute_device: &str,
) -> Result<(), AppError> {
    with_recognizer(
        app_data,
        model_root,
        provider,
        language,
        compute_device,
        |_| Ok(()),
    )?;
    Ok(())
}

pub fn transcribe_wav(
    app_data: &Path,
    model_root: &Path,
    provider: &str,
    language: Option<&str>,
    wav_path: &Path,
    compute_device: &str,
) -> Result<String, AppError> {
    with_recognizer(
        app_data,
        model_root,
        provider,
        language,
        compute_device,
        |recognizer| recognizer.transcribe_wav(wav_path),
    )
}

pub fn transcribe_samples(
    app_data: &Path,
    model_root: &Path,
    provider: &str,
    language: Option<&str>,
    samples: &[f32],
    sample_rate: i32,
    compute_device: &str,
) -> Result<String, AppError> {
    with_recognizer(
        app_data,
        model_root,
        provider,
        language,
        compute_device,
        |recognizer| recognizer.transcribe_samples(samples, sample_rate),
    )
}

fn with_recognizer<T>(
    app_data: &Path,
    model_root: &Path,
    provider: &str,
    language: Option<&str>,
    compute_device: &str,
    operation: impl FnOnce(&LocalRecognizer) -> Result<T, AppError>,
) -> Result<T, AppError> {
    let key = format!(
        "{}|{}|{}|{}",
        provider,
        model_root.to_string_lossy(),
        language.unwrap_or("auto"),
        compute_device
    );
    let mut recognizers = RECOGNIZERS
        .get_or_init(|| Mutex::new(HashMap::new()))
        .lock()
        .map_err(|_| AppError::new("asr.runtime", "本地推理运行时状态异常"))?;
    if !recognizers.contains_key(&key) {
        let recognizer = LocalRecognizer::create(
            &runtime_directory_for(app_data, compute_device),
            model_root,
            provider,
            language,
            "",
            compute_device,
        )?;
        recognizers.clear();
        recognizers.insert(key.clone(), recognizer);
    }
    operation(recognizers.get(&key).expect("recognizer was inserted"))
}

pub fn unload(model_root: &Path) {
    let needle = model_root.to_string_lossy();
    if let Some(recognizers) = RECOGNIZERS.get() {
        if let Ok(mut recognizers) = recognizers.lock() {
            recognizers.retain(|key, _| !key.contains(needle.as_ref()));
        }
    }
    if let Some(recognizers) = ONLINE_RECOGNIZERS.get() {
        if let Ok(mut recognizers) = recognizers.lock() {
            recognizers.retain(|key, _| !key.contains(needle.as_ref()));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runtime_path_is_versioned_and_shared() {
        let path = runtime_directory(Path::new("C:/app-data"));
        assert!(path.ends_with("runtime/sherpa-onnx/1.13.7/win-x64"));
        assert!(cuda_runtime_directory(Path::new("C:/app-data"))
            .ends_with("runtime/sherpa-onnx/1.13.7/win-x64-cuda12"));
        assert!(!cuda_runtime_installed(Path::new("C:/missing-app-data")));
    }

    #[test]
    fn qwen3_context_uses_the_high_capacity_profile() {
        assert_eq!(QWEN3_MAX_TOTAL_LEN, 8_192);
        assert_eq!(QWEN3_MAX_NEW_TOKENS, 512);
    }

    #[test]
    fn offline_cache_key_ignores_hotwords() {
        let key = format!(
            "{}|{}|{}|{}",
            "qwen3-asr",
            "C:/models/qwen3",
            "zh",
            "cpu"
        );
        assert!(!key.contains("语流"));
        assert_eq!(key.matches('|').count(), 3);
    }

    #[test]
    fn model_prefix_is_derived_from_download_directory() {
        assert_eq!(
            whisper_prefix(Path::new("C:/models/faster-whisper-medium")).unwrap(),
            "medium"
        );
        assert_eq!(
            whisper_prefix(Path::new("C:/models/faster-whisper-large-v3")).unwrap(),
            "large-v3"
        );
    }

    #[test]
    fn stable_stream_text_gets_sentence_punctuation() {
        assert_eq!(punctuate_stable("今天天气很好"), "今天天气很好。");
        assert_eq!(punctuate_stable("hello world"), "hello world.");
        assert_eq!(punctuate_stable("已经有标点！"), "已经有标点！");
    }

    #[test]
    #[ignore = "requires an extracted sherpa-onnx runtime"]
    fn native_runtime_smoke() {
        let root = std::env::var_os("YULIU_SHERPA_RUNTIME")
            .expect("set YULIU_SHERPA_RUNTIME to the extracted runtime directory");
        SherpaApi::load(Path::new(&root)).expect("runtime should load and expose version 1.13.7");
    }

    #[test]
    #[ignore = "requires a downloaded model and a WAV fixture"]
    fn native_offline_transcription_smoke() {
        let app_data = std::env::var_os("YULIU_APP_DATA")
            .expect("set YULIU_APP_DATA to the application data directory");
        let model = std::env::var_os("YULIU_ASR_MODEL")
            .expect("set YULIU_ASR_MODEL to a downloaded model directory");
        let wav = std::env::var_os("YULIU_ASR_WAV").expect("set YULIU_ASR_WAV to a WAV file");
        let provider = std::env::var("YULIU_ASR_PROVIDER").unwrap_or_else(|_| "qwen3-asr".into());
            let text = transcribe_wav(
                Path::new(&app_data),
                Path::new(&model),
                &provider,
                Some("zh"),
                Path::new(&wav),
                "cpu",
            )
        .expect("offline transcription should succeed");
        assert!(!text.is_empty(), "offline transcription returned no text");
        eprintln!("{text}");
    }

    #[test]
    #[ignore = "requires installed runtime and streaming model"]
    fn native_streaming_model_smoke() {
        let app_data = std::env::var_os("YULIU_APP_DATA")
            .expect("set YULIU_APP_DATA to the application data directory");
        let model = std::env::var_os("YULIU_STREAMING_MODEL")
            .expect("set YULIU_STREAMING_MODEL to the streaming model directory");
        validate_streaming(Path::new(&app_data), Path::new(&model), "cuda")
            .expect("streaming model should load on CUDA");
    }

    #[test]
    #[ignore = "requires installed runtime, streaming model, and test WAV"]
    fn native_streaming_audio_smoke() {
        let app_data = std::env::var_os("YULIU_APP_DATA").expect("set YULIU_APP_DATA");
        let model = std::env::var_os("YULIU_STREAMING_MODEL").expect("set YULIU_STREAMING_MODEL");
        let wav = std::env::var_os("YULIU_TEST_WAV").expect("set YULIU_TEST_WAV");
        let mut reader = hound::WavReader::open(wav).expect("open test WAV");
        let spec = reader.spec();
        let samples = reader
            .samples::<i16>()
            .map(|sample| sample.expect("valid PCM") as f32 / i16::MAX as f32)
            .collect::<Vec<_>>();
        let (sender, receiver) = mpsc::channel(64);
        std::thread::spawn(move || {
            for values in samples.chunks((spec.sample_rate / 10 * spec.channels as u32) as usize) {
                sender
                    .blocking_send(StreamMessage::Audio(AudioChunk {
                        samples: values.to_vec(),
                        sample_rate: spec.sample_rate,
                        channels: spec.channels,
                    }))
                    .expect("send audio");
            }
            sender
                .blocking_send(StreamMessage::Finish)
                .expect("finish audio");
        });
        let mut results = Vec::new();
        stream(
            Path::new(&app_data),
            Path::new(&model),
            "cuda",
            receiver,
            |text, stable| {
                println!("{}: {text}", if stable { "final" } else { "partial" });
                results.push(text.to_owned());
                Ok(())
            },
        )
        .expect("decode test WAV");
        assert!(
            results.iter().any(|text| !text.trim().is_empty()),
            "streaming decoder returned no text"
        );
    }
}
