import { create } from "zustand";
import { removeHotword, saveHotword, saveSettings } from "./api";
import { DEFAULT_HOTKEYS, mergeHotkeys } from "./hotkeys";
import { closeAppContextMenu } from "./ui/contextMenu";
import type {
  AppSettings,
  Hotword,
  LlmConfig,
  LlmProvider,
  RecordItem,
  RecordingSession,
  Route,
  VoiceProcessPhase,
} from "./types";

interface Store {
  route: Route;
  settingsOpen: boolean;
  settingsTab: string;
  hotwords: Hotword[];
  records: RecordItem[];
  session: RecordingSession | null;
  partialText: string;
  committedText: string;
  audioLevel: number;
  voicePhase: VoiceProcessPhase;
  notice?: string;
  settings: AppSettings;
  setRoute: (route: Route) => void;
  openSettings: (tab?: string) => void;
  closeSettings: () => void;
  setSettingsTab: (tab: string) => void;
  upsertHotword: (item: Hotword) => void;
  deleteHotword: (id: number) => void;
  patchSettings: (
    patch: Partial<Omit<AppSettings, "llm">> & {
      llm?: Partial<Record<LlmProvider, Partial<LlmConfig>>>;
    },
  ) => void;
  replaceSettings: (settings: AppSettings) => void;
  replaceHotwords: (items: Hotword[]) => void;
  replaceRecords: (items: RecordItem[]) => void;
  setSession: (session: RecordingSession | null) => void;
  setVoice: (
    patch: Partial<
      Pick<
        Store,
        | "partialText"
        | "committedText"
        | "audioLevel"
        | "voicePhase"
        | "notice"
      >
    >,
  ) => void;
}

export const defaultSettings: AppSettings = {
  directInsert: true,
  autoFormat: false,
  autoEndOnSilence: true,
  bubbleStyle: "A",
  asrProvider: "openai",
  asrModel: "gpt-4o-mini-transcribe",
  asrBaseUrl: "https://api.openai.com/v1",
  localModelId: undefined,
  localModelPath: undefined,
  localModelEngine: undefined,
  computeDevice: "auto",
  autoStart: false,
  hideOnStart: true,
  sounds: true,
  retentionDays: 30,
  llmEnabled: true,
  llmProvider: "openai",
  llm: {
    openai: {
      provider: "openai",
      model: "gpt-5-mini",
      baseUrl: "https://api.openai.com/v1",
      maxOutputTokens: 2048,
      thinking: false,
      configured: false,
    },
    claude: {
      provider: "claude",
      model: "claude-sonnet-4-5",
      baseUrl: "https://api.anthropic.com",
      maxOutputTokens: 2048,
      thinking: false,
      configured: false,
    },
  },
  hotkeys: { ...DEFAULT_HOTKEYS },
  hotwordsEnabled: true,
  hotwordCaseSensitive: false,
  defaultHotwordPriority: 3,
  defaultPromptId: undefined,
};

export const useAppStore = create<Store>()((set) => ({
      route: "home",
      settingsOpen: false,
      settingsTab: "general",
      hotwords: [],
      records: [],
      session: null,
      partialText: "",
      committedText: "",
      audioLevel: 0,
      voicePhase: "idle",
      settings: defaultSettings,
      setRoute: (route) => {
        closeAppContextMenu();
        set({ route });
      },
      openSettings: (settingsTab = "general") => {
        closeAppContextMenu();
        set({ settingsOpen: true, settingsTab });
      },
      closeSettings: () => {
        closeAppContextMenu();
        set({ settingsOpen: false });
      },
      setSettingsTab: (settingsTab) => {
        closeAppContextMenu();
        set({ settingsTab });
      },
      upsertHotword: (item) =>
        set((s) => {
          void saveHotword(item);
          return { hotwords: s.hotwords.some((h) => h.id === item.id)
            ? s.hotwords.map((h) => (h.id === item.id ? item : h))
            : [item, ...s.hotwords] };
        }),
      deleteHotword: (id) => set((s) => {
        void removeHotword(id);
        return { hotwords: s.hotwords.filter((h) => h.id !== id) };
      }),
      patchSettings: (patch) =>
        set((s) => {
          const previous = s.settings;
          const settings = {
            ...previous,
            ...patch,
            llm: patch.llm
              ? {
                  openai: patch.llm.openai
                    ? { ...previous.llm.openai, ...patch.llm.openai }
                    : previous.llm.openai,
                  claude: patch.llm.claude
                    ? { ...previous.llm.claude, ...patch.llm.claude }
                    : previous.llm.claude,
                }
              : previous.llm,
            hotkeys: mergeHotkeys(patch.hotkeys || previous.hotkeys),
          };
          void saveSettings(settings).catch(() => {
            useAppStore.setState((current) =>
              current.settings === settings ? { settings: previous } : current,
            );
          });
          return { settings };
        }),
      replaceSettings: (settings) => set((state) => ({
        settings: {
          ...state.settings,
          ...settings,
          llm: {
            openai: {
              ...state.settings.llm.openai,
              ...settings.llm?.openai,
              thinking:
                settings.llm?.openai?.thinking ??
                state.settings.llm.openai.thinking ??
                false,
            },
            claude: {
              ...state.settings.llm.claude,
              ...settings.llm?.claude,
              thinking:
                settings.llm?.claude?.thinking ??
                state.settings.llm.claude.thinking ??
                false,
            },
          },
          hotkeys: mergeHotkeys(settings.hotkeys || state.settings.hotkeys),
        },
      })),
      replaceHotwords: (hotwords) => set({ hotwords }),
      replaceRecords: (records) => set({ records }),
      setSession: (session) => set({ session }),
      setVoice: (patch) => set(patch),
    }));
