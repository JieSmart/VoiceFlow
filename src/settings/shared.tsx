import {
  ChartBar,
  GearSix,
  Keyboard,
  Microphone,
  NotePencil,
  ShieldCheck,
  Sparkle,
  Tag,
} from "@phosphor-icons/react";

export const settingsTabs = [
  { id: "general", label: "通用", icon: GearSix },
  { id: "hotkey", label: "快捷键", icon: Keyboard },
  { id: "asr", label: "语音识别", icon: Microphone },
  { id: "llm", label: "大模型", icon: Sparkle },
  { id: "hotword", label: "热词行为", icon: Tag },
  { id: "prompt", label: "提示词", icon: NotePencil },
  { id: "privacy", label: "隐私与存储", icon: ShieldCheck },
  { id: "about", label: "关于", icon: ChartBar },
];
