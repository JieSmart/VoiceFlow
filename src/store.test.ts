import { beforeEach, describe, expect, it, vi } from "vitest";
import { saveSettings } from "./api";
import { defaultSettings, useAppStore } from "./store";

vi.mock("./api", () => ({
  saveSettings: vi.fn(() => Promise.resolve()),
  saveHotword: vi.fn(() => Promise.resolve()),
  removeHotword: vi.fn(() => Promise.resolve()),
}));

describe("application store", () => {
  beforeEach(() => {
    localStorage.clear();
    vi.mocked(saveSettings).mockReset();
    vi.mocked(saveSettings).mockResolvedValue(undefined);
    useAppStore.setState({
      route: "home",
      settingsOpen: false,
      hotwords: [],
      partialText: "",
      committedText: "",
      audioLevel: 0,
      voicePhase: "idle",
      settings: defaultSettings,
    });
  });

  it("creates, updates and removes hotwords", () => {
    const item = {
      id: 42,
      word: "feishu",
      category: "replace" as const,
      behavior: "直接替换",
      replacement: "飞书文档",
      priority: 3,
      apps: ["全部应用"],
      hits: 0,
      lastUsed: "从未",
      enabled: true,
      note: "",
    };
    useAppStore.getState().upsertHotword(item);
    expect(useAppStore.getState().hotwords).toEqual([item]);

    useAppStore.getState().upsertHotword({ ...item, hits: 1 });
    expect(useAppStore.getState().hotwords[0].hits).toBe(1);

    useAppStore.getState().deleteHotword(item.id);
    expect(useAppStore.getState().hotwords).toEqual([]);
  });

  it("keeps voice state updates independent", () => {
    useAppStore.getState().setVoice({
      partialText: "语流",
      audioLevel: 0.72,
      voicePhase: "recognizing",
    });
    expect(useAppStore.getState().partialText).toBe("语流");
    expect(useAppStore.getState().committedText).toBe("");
    expect(useAppStore.getState().audioLevel).toBe(0.72);
    expect(useAppStore.getState().voicePhase).toBe("recognizing");
  });

  it("uses the simplified direct-insert defaults and preserves partial settings", () => {
    expect(defaultSettings.autoEndOnSilence).toBe(true);
    expect(defaultSettings.bubbleStyle).toBe("A");
    expect(defaultSettings.llmEnabled).toBe(true);
    expect(defaultSettings.llm.openai.thinking).toBe(false);
    expect(defaultSettings.llm.claude.thinking).toBe(false);
    expect(defaultSettings.directInsert).toBe(true);
    expect(defaultSettings.autoFormat).toBe(false);
    expect(defaultSettings.hideOnStart).toBe(true);
    useAppStore.getState().replaceSettings({ directInsert: false } as typeof defaultSettings);
    expect(useAppStore.getState().settings.autoEndOnSilence).toBe(true);
    expect(useAppStore.getState().settings.directInsert).toBe(false);
    useAppStore.getState().replaceSettings({
      ...defaultSettings,
      autoEndOnSilence: false,
    });
    expect(useAppStore.getState().settings.autoEndOnSilence).toBe(false);
    useAppStore.getState().replaceSettings({
      ...defaultSettings,
      autoEndOnSilence: true,
    });
  });

  it("keeps the previous settings when saving fails", async () => {
    vi.mocked(saveSettings).mockRejectedValueOnce(new Error("hotkey.register"));
    useAppStore.getState().patchSettings({ microphoneId: "USB Mic" });
    expect(useAppStore.getState().settings.microphoneId).toBe("USB Mic");
    await vi.waitFor(() => {
      expect(useAppStore.getState().settings.microphoneId).toBeUndefined();
    });
  });

  it("deep-merges hotkeys when replacing partial settings", () => {
    useAppStore.getState().replaceSettings({
      hotkeys: { wake: "Ctrl+Alt+Q" },
    } as typeof defaultSettings);
    expect(useAppStore.getState().settings.hotkeys.wake).toBe("Ctrl+Alt+Q");
    expect(useAppStore.getState().settings.hotkeys.panel).toBe("Ctrl+Shift+V");
    expect(useAppStore.getState().settings.hotwordsEnabled).toBe(true);
  });

  it("keeps the other llm provider when replacing a nested openai patch", () => {
    useAppStore.getState().replaceSettings({
      llm: { openai: { model: "gpt-x" } },
    } as typeof defaultSettings);
    expect(useAppStore.getState().settings.llm.openai.model).toBe("gpt-x");
    expect(useAppStore.getState().settings.llm.openai.thinking).toBe(false);
    expect(useAppStore.getState().settings.llm.claude.model).toBe("claude-sonnet-4-5");
  });

  it("replaces one llm provider object without dropping the other", () => {
    useAppStore.getState().patchSettings({
      llm: {
        openai: {
          provider: "openai",
          model: "gpt-x",
          baseUrl: "https://api.openai.com/v1",
          maxOutputTokens: 2048,
          thinking: true,
          configured: true,
        },
      },
    });
    expect(useAppStore.getState().settings.llm.openai.model).toBe("gpt-x");
    expect(useAppStore.getState().settings.llm.openai.thinking).toBe(true);
    expect(useAppStore.getState().settings.llm.claude.model).toBe("claude-sonnet-4-5");
  });
});
