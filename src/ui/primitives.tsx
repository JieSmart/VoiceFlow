import type { ReactNode } from "react";
import { Minus, Plus, X } from "@phosphor-icons/react";
import clsx from "clsx";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { hideMainWindow, isTauri } from "../api";

export function WindowControls() {
  const act = async (kind: "close" | "min" | "max") => {
    if (!isTauri()) return;
    const win = getCurrentWindow();
    if (kind === "close") {
      await hideMainWindow();
    } else if (kind === "min") {
      await win.minimize();
    } else {
      await win.toggleMaximize();
    }
  };
  return (
    <div className="traffic" aria-label="窗口控制">
      <button
        className="traffic-dot close"
        title="隐藏到托盘"
        onClick={() => act("close")}
      >
        <X weight="bold" />
      </button>
      <button
        className="traffic-dot min"
        title="最小化"
        onClick={() => act("min")}
      >
        <Minus weight="bold" />
      </button>
      <button
        className="traffic-dot max"
        title="最大化"
        onClick={() => act("max")}
      >
        <Plus weight="bold" />
      </button>
    </div>
  );
}

const WAVE_BARS = [
  { x: 213, y: 430.3, h: 163.4 },
  { x: 343, y: 378.7, h: 266.6 },
  { x: 473, y: 297, h: 430 },
  { x: 603, y: 378.7, h: 266.6 },
  { x: 733, y: 430.3, h: 163.4 },
] as const;

export function WaveLogo({ size = 34 }: { size?: number }) {
  return (
    <div className="wave-logo" style={{ width: size, height: size }} aria-hidden>
      <svg viewBox="0 0 1024 1024" width="100%" height="100%">
        {WAVE_BARS.map((bar) => (
          <rect
            key={bar.x}
            x={bar.x}
            y={bar.y}
            width={78}
            height={bar.h}
            rx={39}
            fill="currentColor"
          />
        ))}
      </svg>
    </div>
  );
}

export function Kbd({ children }: { children: ReactNode }) {
  return <span className="kbd">{children}</span>;
}

export function Toggle({
  on,
  onChange,
}: {
  on: boolean;
  onChange: (v: boolean) => void;
}) {
  return (
    <button
      role="switch"
      aria-checked={on}
      className={clsx("toggle", on && "on")}
      onClick={() => onChange(!on)}
    >
      <i />
    </button>
  );
}

export function SettingsHeader({ title, desc }: { title: string; desc: string }) {
  return (
    <div className="settings-title">
      <h2>{title}</h2>
      <p>{desc}</p>
    </div>
  );
}

export function SettingRow({
  title,
  desc,
  children,
}: {
  title: string;
  desc?: string;
  children: ReactNode;
}) {
  return (
    <div className="setting-row">
      <div>
        <b>{title}</b>
        {desc && <small>{desc}</small>}
      </div>
      <div>{children}</div>
    </div>
  );
}

export function formatBytes(bytes: number) {
  if (bytes < 1024) return `${bytes} B`;
  const units = ["KB", "MB", "GB", "TB"];
  let value = bytes / 1024;
  let unit = units[0];
  for (let index = 1; index < units.length && value >= 1024; index += 1) {
    value /= 1024;
    unit = units[index];
  }
  return `${value >= 100 ? value.toFixed(0) : value.toFixed(1)} ${unit}`;
}

export function formatRecordingTime(seconds: number) {
  const whole = Math.max(0, Math.floor(seconds));
  return `${Math.floor(whole / 60)}:${String(whole % 60).padStart(2, "0")}`;
}
