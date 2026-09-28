import { lazy, Suspense } from "react";
import { X } from "@phosphor-icons/react";
import { useAppStore } from "../store";
import { WaveLogo } from "../ui/primitives";
import { settingsTabs } from "./shared";

const GeneralSettings = lazy(() =>
  import("./GeneralSettings").then((module) => ({ default: module.GeneralSettings })),
);
const HotkeySettings = lazy(() =>
  import("./HotkeySettings").then((module) => ({ default: module.HotkeySettings })),
);
const AsrSettings = lazy(() =>
  import("./AsrSettings").then((module) => ({ default: module.AsrSettings })),
);
const LlmSettings = lazy(() =>
  import("./LlmSettings").then((module) => ({ default: module.LlmSettings })),
);
const HotwordBehaviorSettings = lazy(() =>
  import("./HotwordBehaviorSettings").then((module) => ({
    default: module.HotwordBehaviorSettings,
  })),
);
const PromptSettings = lazy(() =>
  import("./PromptSettings").then((module) => ({ default: module.PromptSettings })),
);
const PrivacySettings = lazy(() =>
  import("./PrivacySettings").then((module) => ({ default: module.PrivacySettings })),
);
const AboutSettings = lazy(() =>
  import("./AboutSettings").then((module) => ({ default: module.AboutSettings })),
);

export function SettingsModal() {
  const close = useAppStore((s) => s.closeSettings),
    tab = useAppStore((s) => s.settingsTab),
    setTab = useAppStore((s) => s.setSettingsTab);
  return (
    <div
      className="backdrop"
      onMouseDown={(event) => {
        if (event.target === event.currentTarget) close();
      }}
    >
      <div className="settings-modal" onMouseDown={(event) => event.stopPropagation()}>
        <div className="settings-modal-head">
          <div>
            <WaveLogo size={28} />
            <b>语流设置</b>
          </div>
          <button className="icon-button" onClick={close}>
            <X />
          </button>
        </div>
        <div className="settings-layout">
          <aside>
            {settingsTabs.map(({ id, label, icon: Icon }) => (
              <button
                className={tab === id ? "active" : ""}
                onClick={() => setTab(id)}
                key={id}
              >
                <Icon />
                <span>{label}</span>
              </button>
            ))}
          </aside>
          <main>
            <Suspense fallback={null}>
              {tab === "general" ? (
                <GeneralSettings />
              ) : tab === "hotkey" ? (
                <HotkeySettings />
              ) : tab === "asr" ? (
                <AsrSettings />
              ) : tab === "llm" ? (
                <LlmSettings />
              ) : tab === "hotword" ? (
                <HotwordBehaviorSettings />
              ) : tab === "prompt" ? (
                <PromptSettings />
              ) : tab === "privacy" ? (
                <PrivacySettings />
              ) : (
                <AboutSettings />
              )}
            </Suspense>
          </main>
        </div>
      </div>
    </div>
  );
}
