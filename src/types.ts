export type Route = "home" | "hotwords" | "records";
export type SessionPhase =
  | "idle"
  | "capturing"
  | "streaming"
  | "finalizing"
  | "inserted"
  | "formatting"
  | "replaced"
  | "completed"
  | "focus_lost"
  | "cancelled"
  | "failed";

export type VoiceProcessPhase =
  | "idle"
  | "waking"
  | "listening"
  | "recognizing"
  | "transforming"
  | "inserting"
  | "completed"
  | "failed";

export interface VoiceProcessStatus {
  sessionId?: string;
  phase: VoiceProcessPhase;
  message?: string;
}

export interface Hotword {
  id: number;
  word: string;
  category: "replace" | "cmd" | "industry" | "contact" | "brand";
  behavior: string;
  replacement?: string;
  priority: number;
  apps: string[];
  hits: number;
  lastUsed: string;
  enabled: boolean;
  note: string;
}
export interface RecordItem {
  id: string;
  startedAt: string;
  app: string;
  duration: number;
  chars: number;
  hotwords: string[];
  ai: boolean;
  model?: string;
  llmProvider?: LlmProvider;
  text: string;
  formattedText?: string;
}
export interface FocusTarget {
  windowId: number;
  processId: number;
  title: string;
  capturedAt: string;
}
export interface RecordingSession {
  id: string;
  state: SessionPhase;
  target: FocusTarget;
  startedAt: string;
  committedText: string;
}
export interface RecordingReview {
  sessionId: string;
  text: string;
  duration: number;
  hotwords: string[];
  engine: string;
  model?: string;
}
export interface TranscriptSegment {
  text: string;
  stable: boolean;
  startMs: number;
  endMs: number;
  confidence?: number;
}
export interface AudioDevice {
  id: string;
  name: string;
  isDefault: boolean;
}
export type LocalModelStatus =
  | "idle"
  | "downloading"
  | "paused"
  | "done"
  | "error";

export interface LocalModelInfo {
  id: string;
  engine: "qwen3-asr" | "faster-whisper" | "sherpa-streaming";
  name: string;
  size: string;
  languages: string;
  speed: string;
  description: string;
  status: LocalModelStatus;
  progress: number;
  path?: string;
  message?: string;
  downloadedBytes?: number;
  totalBytes?: number;
  bytesPerSecond?: number;
}

export interface LocalModelProgress {
  id: string;
  status: LocalModelStatus;
  progress: number;
  path?: string;
  message?: string;
  downloadedBytes?: number;
  totalBytes?: number;
  bytesPerSecond?: number;
}
export interface AppError {
  code: string;
  message: string;
  retryable: boolean;
  fallback?: string;
}
export type LlmProvider = "openai" | "claude";
export type { Hotkeys, HotkeyMode } from "./hotkeys";
export type { PromptTemplate } from "./promptTemplates";

export interface LlmConfig {
  provider: LlmProvider;
  model: string;
  baseUrl: string;
  maxOutputTokens: number;
  thinking: boolean;
  configured: boolean;
}
export interface AppSettings {
  directInsert: boolean;
  autoFormat: boolean;
  autoEndOnSilence: boolean;
  bubbleStyle: "A" | "B" | "C";
  asrProvider: "openai" | "xunfei" | "tencent" | "sherpa-streaming" | "qwen3-asr" | "faster-whisper";
  asrModel: string;
  asrBaseUrl: string;
  localModelId?: string;
  localModelPath?: string;
  localModelEngine?: "qwen3-asr" | "faster-whisper" | "sherpa-streaming";
  computeDevice: "auto" | "cpu" | "cuda";
  microphoneId?: string;
  autoStart: boolean;
  hideOnStart: boolean;
  sounds: boolean;
  retentionDays: number;
  llmEnabled: boolean;
  llmProvider: LlmProvider;
  llm: Record<LlmProvider, LlmConfig>;
  hotkeys: import("./hotkeys").Hotkeys;
  hotwordsEnabled: boolean;
  hotwordCaseSensitive: boolean;
  defaultHotwordPriority: number;
  defaultPromptId?: number;
}

export interface ComputeStatus {
  preference: "auto" | "cpu" | "cuda";
  active: "cpu" | "cuda";
  gpuName?: string;
  cudaRuntimeAvailable: boolean;
  message: string;
}

export interface DataDirectoryInfo {
  currentPath: string;
  defaultPath: string;
  isCustom: boolean;
}

export interface DataDirectoryChoice {
  parentPath: string;
  targetPath: string;
}

export interface DataMigrationResult {
  oldPath: string;
  newPath: string;
  restartRequired: boolean;
}
