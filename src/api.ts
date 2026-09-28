import { invoke } from "@tauri-apps/api/core";
import { emit, listen, type UnlistenFn } from "@tauri-apps/api/event";
import type {
  AudioDevice,
  FocusTarget,
  LocalModelInfo,
  Hotword,
  RecordItem,
  AppSettings,
  LlmProvider,
  RecordingSession,
  RecordingReview,
  TranscriptSegment,
  VoiceProcessStatus,
  ComputeStatus,
  DataDirectoryChoice,
  DataDirectoryInfo,
  DataMigrationResult,
  PromptTemplate,
} from "./types";

export const isTauri = () => "__TAURI_INTERNALS__" in window;
const demoTarget: FocusTarget = {
  windowId: 0,
  processId: 0,
  title: "浏览器预览",
  capturedAt: new Date().toISOString(),
};

export async function captureFocusTarget(): Promise<FocusTarget | null> {
  if (!isTauri()) return demoTarget;
  return invoke<FocusTarget>("capture_focus_target");
}

export async function startRecording(target?: FocusTarget | null) {
  if (!isTauri())
    return {
      id: crypto.randomUUID(),
      state: "capturing",
      target: demoTarget,
      startedAt: new Date().toISOString(),
      committedText: "",
    } as RecordingSession;
  return invoke<RecordingSession>("start_recording", { target: target || null });
}
export async function stopRecording(sessionId: string) {
  if (!isTauri()) return;
  return invoke<void>("stop_recording", { sessionId });
}
export async function stopRecordingForReview(
  sessionId: string,
  transcript?: string,
  showReview = true,
) {
  if (!isTauri()) {
    return {
      sessionId,
      text: transcript || "",
      duration: 0,
      hotwords: [],
      engine: "浏览器模拟",
    } as RecordingReview;
  }
  return invoke<RecordingReview>("stop_recording_for_review", {
    sessionId,
    transcript: transcript || null,
    showReview,
  });
}
export async function commitRecordingReview(request: {
  sessionId: string;
  text: string;
  ai: boolean;
  llmProvider?: LlmProvider;
  llmModel?: string;
}) {
  if (!isTauri()) return;
  return invoke("commit_recording_review", request);
}
export async function cancelRecording(sessionId: string) {
  if (!isTauri()) return;
  return invoke("cancel_recording", { sessionId });
}
export async function recoverFailedSession(sessionId: string) {
  if (!isTauri()) return;
  return invoke("recover_failed_session", { sessionId });
}
export async function injectSegment(
  sessionId: string,
  segment: TranscriptSegment,
) {
  if (!isTauri())
    return { sessionId, state: "inserted", committedText: segment.text };
  return invoke("inject_segment", { sessionId, segment });
}
export async function replaceFinalTranscript(
  sessionId: string,
  segment: TranscriptSegment,
) {
  if (!isTauri())
    return { sessionId, state: "replaced", committedText: segment.text };
  return invoke("replace_final_transcript", { sessionId, segment });
}
export async function transcribeFinal(sessionId: string) {
  if (!isTauri()) return null;
  return invoke<TranscriptSegment | null>("transcribe_final", { sessionId });
}

export async function getSettings(): Promise<Record<string, unknown>> {
  if (!isTauri()) return {};
  return invoke("get_settings");
}

export async function saveSettings(settings: AppSettings) {
  if (!isTauri()) return;
  return invoke("set_setting", { key: "appSettings", value: settings });
}

export async function getDataDirectory(): Promise<DataDirectoryInfo> {
  if (!isTauri())
    return {
      currentPath: "C:\\Users\\用户\\AppData\\Roaming\\com.yuliu.voiceinput",
      defaultPath: "C:\\Users\\用户\\AppData\\Roaming\\com.yuliu.voiceinput",
      isCustom: false,
    };
  return invoke("get_data_directory");
}

export async function chooseDataDirectory(): Promise<DataDirectoryChoice | null> {
  if (!isTauri()) return null;
  return invoke("choose_data_directory");
}

export async function migrateDataDirectory(parentPath: string): Promise<DataMigrationResult> {
  if (!isTauri()) {
    return {
      oldPath: "应用默认数据目录",
      newPath: `${parentPath}\\语流`,
      restartRequired: true,
    };
  }
  return invoke("migrate_data_directory", { parentPath });
}

export async function quitForDataDirectoryRestart() {
  if (!isTauri()) return;
  return invoke("quit_for_data_directory_restart");
}

export async function listHotwords(): Promise<Hotword[]> {
  if (!isTauri()) return [];
  return invoke("list_hotwords");
}

export async function saveHotword(hotword: Hotword) {
  if (!isTauri()) return;
  return invoke("upsert_hotword", { hotword });
}

export async function removeHotword(id: number) {
  if (!isTauri()) return;
  return invoke("remove_hotword", { id });
}

export async function listRecords(): Promise<RecordItem[]> {
  if (!isTauri()) return [];
  return invoke("list_records");
}

export async function clearRecords() {
  if (!isTauri()) return;
  return invoke("clear_records");
}

const demoPrompts: PromptTemplate[] = [
  {
    id: 1,
    name: "会议纪要整理",
    triggerWord: "纪要",
    prompt: "整理成纪要：{{text}}",
    enabled: true,
    isDefault: true,
  },
];

export async function listPrompts(): Promise<PromptTemplate[]> {
  if (!isTauri()) return demoPrompts;
  return invoke("list_prompts");
}

export async function savePrompt(prompt: PromptTemplate) {
  if (!isTauri()) return { ...prompt, id: prompt.id || Date.now() };
  return invoke<PromptTemplate>("upsert_prompt", { prompt });
}

export async function removePrompt(id: number) {
  if (!isTauri()) return;
  return invoke("remove_prompt", { id });
}
export async function audioDevices(): Promise<AudioDevice[]> {
  if (!isTauri())
    return [{ id: "default", name: "默认麦克风", isDefault: true }];
  return invoke("list_audio_devices");
}
const demoLocalModels: LocalModelInfo[] = [
  { id: "sherpa-streaming-zipformer-zh-en", engine: "sherpa-streaming", name: "Sherpa Zipformer 中英流式", size: "约 190 MB", languages: "中文 / 英文", speed: "实时", description: "真正的本地增量识别，边说边出字", status: "idle", progress: 0 },
  { id: "qwen3-asr-1.7b", engine: "qwen3-asr", name: "Qwen3-ASR 1.7B INT8", size: "约 2.4 GB", languages: "30 种语言 / 22 种中文方言", speed: "中", description: "高准确率多语言与中文方言识别", status: "idle", progress: 0 },
  { id: "faster-whisper-medium", engine: "faster-whisper", name: "Whisper medium", size: "约 1.5 GB", languages: "99 种语言", speed: "中快", description: "准确率与资源占用均衡", status: "idle", progress: 0 },
  { id: "faster-whisper-large-v3", engine: "faster-whisper", name: "Whisper large-v3", size: "约 3.1 GB", languages: "99 种语言", speed: "中", description: "多语言高准确率模型", status: "idle", progress: 0 },
];
export async function listLocalModels(): Promise<LocalModelInfo[]> {
  if (!isTauri()) return demoLocalModels;
  return invoke("list_local_models");
}
export async function localModelsDirectory(): Promise<string> {
  if (!isTauri()) return "应用数据目录/models";
  return invoke("local_models_directory");
}
export async function downloadLocalModel(modelId: string) {
  if (!isTauri()) return;
  return invoke("download_local_model", { modelId });
}
export async function pauseLocalModelDownload(modelId: string) {
  if (!isTauri()) return;
  return invoke("pause_local_model_download", { modelId });
}
export async function deleteLocalModel(modelId: string) {
  if (!isTauri()) return;
  return invoke("delete_local_model", { modelId });
}
export async function setSecret(name: string, value: string) {
  if (!isTauri()) return;
  return invoke("set_secret", { name, value });
}
export async function secretConfigured(name: string) {
  if (!isTauri()) return false;
  return invoke<boolean>("secret_configured", { name });
}
export async function testLlm(
  provider: LlmProvider,
  model: string,
  baseUrl?: string,
) {
  if (!isTauri()) {
    await new Promise((r) => setTimeout(r, 700));
    return;
  }
  return invoke("test_llm_provider", { request: { provider, model, baseUrl } });
}
export async function listLlmModels(provider: LlmProvider, baseUrl?: string) {
  if (!isTauri()) return [] as Record<string, unknown>[];
  return invoke<Record<string, unknown>[]>("list_llm_models", {
    provider,
    baseUrl: baseUrl || null,
  });
}
export async function testAsr(config: {
  provider: string;
  model?: string;
  baseUrl?: string;
  language?: string;
  computeDevice?: "auto" | "cpu" | "cuda";
}) {
  if (!isTauri()) {
    await new Promise((r) => setTimeout(r, 400));
    return;
  }
  return invoke("test_asr_provider", { config });
}
export async function getComputeStatus(): Promise<ComputeStatus> {
  if (!isTauri()) return { preference: "auto", active: "cpu", cudaRuntimeAvailable: false, message: "浏览器预览使用 CPU 模拟" };
  return invoke<ComputeStatus>("get_compute_status");
}
export async function streamLlm(request: {
  provider: LlmProvider;
  model: string;
  baseUrl?: string;
  instructions: string;
  input: string;
  maxOutputTokens: number;
  thinking?: boolean;
}) {
  if (!isTauri()) return request.input;
  return invoke<string>("stream_llm", { request });
}
export async function onEvent<T>(
  name: string,
  handler: (payload: T) => void,
): Promise<UnlistenFn> {
  if (!isTauri()) return () => {};
  return listen<T>(name, (e) => handler(e.payload));
}

export async function publishVoiceStatus(status: VoiceProcessStatus) {
  if (!isTauri()) return;
  await emit("voice-process-status", status);
}

export async function requestVoiceStop(sessionId: string) {
  if (!isTauri()) return;
  await emit("shortcut-stop-requested", { sessionId });
}

export async function writeDiagnosticLog(
  level: "info" | "warn" | "error",
  event: string,
  sessionId?: string,
  details: Record<string, unknown> = {},
) {
  if (!isTauri()) return;
  try {
    await invoke("write_diagnostic_log", {
      level,
      event,
      sessionId: sessionId || null,
      details,
    });
  } catch {
    // Diagnostics must never interrupt the voice workflow.
  }
}

export async function diagnosticLogPath(): Promise<string> {
  if (!isTauri()) return "应用数据目录/logs/yuliu.log";
  return invoke("diagnostic_log_path");
}

export async function restartApplication() {
  if (!isTauri()) return;
  return invoke("restart_application");
}

export async function quitApplication() {
  if (!isTauri()) return;
  return invoke("quit_application");
}

export async function hideMainWindow() {
  if (!isTauri()) return;
  return invoke("hide_main_window");
}

export async function closeBubble() {
  if (!isTauri()) return;
  return invoke("close_bubble");
}
