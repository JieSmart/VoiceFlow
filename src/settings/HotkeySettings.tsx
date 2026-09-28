import { useEffect, useState } from "react";
import clsx from "clsx";
import {
  acceleratorFromEvent,
  acceleratorParts,
  HOTKEY_LABELS,
  hotkeyConflicts,
  mergeHotkeys,
  type HotkeyId,
} from "../hotkeys";
import { useAppStore } from "../store";
import type { Hotkeys } from "../types";
import { Kbd, SettingsHeader, SettingRow } from "../ui/primitives";

function KeyRecorder({
  value,
  onChange,
}: {
  value: string;
  onChange: (value: string) => void;
}) {
  const [listening, setListening] = useState(false);
  useEffect(() => {
    if (!listening) return undefined;
    const onKeyDown = (event: KeyboardEvent) => {
      event.preventDefault();
      if (event.key === "Escape") {
        setListening(false);
        return;
      }
      const next = acceleratorFromEvent(event);
      if (!next) return;
      onChange(next);
      setListening(false);
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [listening, onChange]);
  return (
    <div className={clsx("key-recorder", listening && "listening")}>
      {listening ? (
        <span>请按下新组合键…</span>
      ) : (
        <>
          <span className="shortcut">
            {acceleratorParts(value).map((part) => (
              <Kbd key={part}>{part}</Kbd>
            ))}
          </span>
          <button className="link-button" onClick={() => setListening(true)}>
            重新录制
          </button>
        </>
      )}
    </div>
  );
}

export function HotkeySettings() {
  const settings = useAppStore((s) => s.settings);
  const patch = useAppStore((s) => s.patchSettings);
  const hotkeys = mergeHotkeys(settings.hotkeys);
  const conflicts = hotkeyConflicts(hotkeys);
  const setHotkey = (id: HotkeyId, value: string) => {
    const next = { ...hotkeys, [id]: value } as Hotkeys;
    if (hotkeyConflicts(next).length) {
      patch({ hotkeys: next });
      return;
    }
    patch({ hotkeys: next });
  };
  return (
    <>
      <SettingsHeader title="快捷键" desc="录制后立即注册为系统全局热键" />
      <div className="settings-card">
        {(
          [
            ["wake", "按下开始，再按结束；按住说话模式下松开即结束"],
            ["panel", "从托盘态呼出或隐藏主窗口"],
            ["ai", "审阅结果时触发或插入 AI 格式化文本"],
            ["mute", "开关热词引擎，与设置里的启用热词同一项，重启后仍保持"],
          ] as const
        ).map(([id, desc]) => (
          <SettingRow key={id} title={HOTKEY_LABELS[id]} desc={desc}>
            <KeyRecorder value={hotkeys[id]} onChange={(value) => setHotkey(id, value)} />
          </SettingRow>
        ))}
        <SettingRow title="唤醒行为" desc="切换模式按一次开始再按一次结束；按住说话松开即结束">
          <div className="segment">
            {(["toggle", "hold"] as const).map((mode) => (
              <button
                key={mode}
                className={hotkeys.mode === mode ? "active" : ""}
                onClick={() => patch({ hotkeys: { ...hotkeys, mode } })}
              >
                {mode === "toggle" ? "切换模式" : "按住说话"}
              </button>
            ))}
          </div>
        </SettingRow>
        {hotkeys.mode === "hold" && (
          <SettingRow title="按住最短时长" desc="短于该时长的按压会取消本次录音">
            <div className="inline-secret">
              <input
                type="range"
                min={100}
                max={500}
                step={10}
                value={hotkeys.holdMs}
                onChange={(e) =>
                  patch({ hotkeys: { ...hotkeys, holdMs: Number(e.target.value) } })
                }
              />
              <span>{hotkeys.holdMs} ms</span>
            </div>
          </SettingRow>
        )}
      </div>
      {conflicts.length > 0 && (
        <div className="settings-card spaced storage-result error">
          {conflicts.map((item) => (
            <div key={item}>{item}</div>
          ))}
        </div>
      )}
    </>
  );
}
