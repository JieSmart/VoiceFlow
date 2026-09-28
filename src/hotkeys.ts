export const DEFAULT_HOTKEYS = {
  wake: "Ctrl+Shift+Space",
  panel: "Ctrl+Shift+V",
  ai: "Ctrl+Alt+G",
  mute: "Ctrl+Shift+M",
  mode: "toggle" as const,
  holdMs: 220,
};

export type HotkeyId = "wake" | "panel" | "ai" | "mute";
export type HotkeyMode = "toggle" | "hold";

export interface Hotkeys {
  wake: string;
  panel: string;
  ai: string;
  mute: string;
  mode: HotkeyMode;
  holdMs: number;
}

const MOD_ORDER = ["Ctrl", "Shift", "Alt", "Meta"] as const;
const RESERVED_KEYS = new Set(["escape", "enter", "return"]);

export const HOTKEY_LABELS: Record<HotkeyId, string> = {
  wake: "唤醒语音输入",
  panel: "打开主面板",
  ai: "AI 格式化",
  mute: "开关热词",
};

function normalizeMod(part: string) {
  const value = part.trim().toLowerCase();
  if (value === "control" || value === "ctrl" || value === "ctl") return "Ctrl";
  if (value === "shift") return "Shift";
  if (value === "alt" || value === "option") return "Alt";
  if (value === "meta" || value === "super" || value === "cmd" || value === "command" || value === "win")
    return "Meta";
  return "";
}

function normalizeKey(part: string) {
  const value = part.trim();
  if (!value) return "";
  if (value.toLowerCase() === " ") return "Space";
  if (value.toLowerCase() === "space") return "Space";
  if (value.length === 1) return value.toUpperCase();
  return value[0].toUpperCase() + value.slice(1);
}

export function normalizeAccelerator(value: string) {
  const parts = value.split("+").map((part) => part.trim()).filter(Boolean);
  if (!parts.length) return "";
  const key = normalizeKey(parts[parts.length - 1] || "");
  const mods = new Set(
    parts.slice(0, -1).map(normalizeMod).filter(Boolean),
  );
  if (normalizeMod(key)) return "";
  return [...MOD_ORDER.filter((mod) => mods.has(mod)), key].filter(Boolean).join("+");
}

export function acceleratorParts(value: string) {
  return normalizeAccelerator(value).split("+").filter(Boolean);
}

export function formatWakeHint(wake: string) {
  return acceleratorParts(wake).join(" + ") || "Ctrl + Shift + Space";
}

export function hasModifier(value: string) {
  const parts = acceleratorParts(value);
  return parts.length > 1 && MOD_ORDER.some((mod) => parts.includes(mod));
}

export function isReservedAccelerator(value: string) {
  const parts = acceleratorParts(value);
  const key = parts[parts.length - 1]?.toLowerCase();
  return !!key && RESERVED_KEYS.has(key);
}

export function keyNameFromEvent(event: KeyboardEvent) {
  if (event.code === "Space" || event.key === " ") return "Space";
  if (event.key === "Escape") return "Escape";
  if (event.key === "Enter") return "Enter";
  if (/^F\d{1,2}$/i.test(event.key)) return event.key.toUpperCase();
  if (event.key.length === 1) return event.key.toUpperCase();
  if (["Control", "Shift", "Alt", "Meta"].includes(event.key)) return null;
  return event.key.length <= 16 ? event.key : null;
}

export function acceleratorFromEvent(event: KeyboardEvent) {
  const key = keyNameFromEvent(event);
  if (!key) return null;
  const mods: string[] = [];
  if (event.ctrlKey) mods.push("Ctrl");
  if (event.shiftKey) mods.push("Shift");
  if (event.altKey) mods.push("Alt");
  if (event.metaKey) mods.push("Meta");
  return normalizeAccelerator([...mods, key].join("+"));
}

export function mergeHotkeys(value?: Partial<Hotkeys> | null): Hotkeys {
  const holdMs = Number(value?.holdMs);
  return {
    wake: normalizeAccelerator(value?.wake || DEFAULT_HOTKEYS.wake) || DEFAULT_HOTKEYS.wake,
    panel: normalizeAccelerator(value?.panel || DEFAULT_HOTKEYS.panel) || DEFAULT_HOTKEYS.panel,
    ai: normalizeAccelerator(value?.ai || DEFAULT_HOTKEYS.ai) || DEFAULT_HOTKEYS.ai,
    mute: normalizeAccelerator(value?.mute || DEFAULT_HOTKEYS.mute) || DEFAULT_HOTKEYS.mute,
    mode: value?.mode === "hold" ? "hold" : "toggle",
    holdMs: Number.isFinite(holdMs) ? Math.min(500, Math.max(100, Math.round(holdMs))) : DEFAULT_HOTKEYS.holdMs,
  };
}

export function hotkeyConflicts(hotkeys: Hotkeys) {
  const ids: HotkeyId[] = ["wake", "panel", "ai", "mute"];
  const errors: string[] = [];
  const seen = new Map<string, HotkeyId>();
  for (const id of ids) {
    const accelerator = normalizeAccelerator(hotkeys[id]);
    if (!hasModifier(accelerator)) {
      errors.push(`${HOTKEY_LABELS[id]}需要包含修饰键`);
    }
    if (isReservedAccelerator(accelerator)) {
      errors.push(`${HOTKEY_LABELS[id]}不能使用 Enter 或 Escape`);
    }
    const previous = seen.get(accelerator);
    if (previous) {
      errors.push(`${HOTKEY_LABELS[id]}与${HOTKEY_LABELS[previous]}冲突`);
    } else if (accelerator) {
      seen.set(accelerator, id);
    }
  }
  return errors;
}
