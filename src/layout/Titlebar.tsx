import { Keyboard } from "@phosphor-icons/react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { isTauri } from "../api";
import { acceleratorParts } from "../hotkeys";
import { pageMeta } from "./pageMeta";
import { useAppStore } from "../store";
import { Kbd, WindowControls } from "../ui/primitives";

export function Titlebar() {
  const [title] = pageMeta[useAppStore((s) => s.route)];
  const settings = useAppStore((s) => s.settings);
  return (
    <header
      className="titlebar"
      data-tauri-drag-region
      onDoubleClick={() => isTauri() && getCurrentWindow().toggleMaximize()}
    >
      <WindowControls />
      <div className="window-title" data-tauri-drag-region>
        <b>语流</b>
        <span>—</span>
        <span>{title}</span>
      </div>
      <div className="title-actions">
        <span className="status-chip">
          <i />
          {settings.asrProvider === "openai"
            ? "OpenAI Realtime · 云端流式"
            : settings.asrProvider === "xunfei"
              ? "讯飞听写 · 云端流式"
              : settings.asrProvider === "tencent"
                ? "腾讯云 ASR · 云端流式"
                : settings.asrProvider === "sherpa-streaming"
                  ? "Sherpa Zipformer · 本地流式"
                  : settings.asrProvider === "qwen3-asr"
                    ? "Qwen3-ASR 1.7B · 本地离线"
                    : settings.asrProvider === "faster-whisper"
                      ? "Whisper · 本地离线"
                      : settings.asrProvider}
        </span>
        <span className="shortcut">
          <Keyboard size={14} />
          {acceleratorParts(settings.hotkeys.wake).map((part) => (
            <Kbd key={part}>{part}</Kbd>
          ))}
        </span>
      </div>
    </header>
  );
}
