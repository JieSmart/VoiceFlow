import { useEffect } from "react";
import {
  getSettings,
  isTauri,
  listHotwords,
  listRecords,
  onEvent,
  saveSettings,
} from "../api";
import { useAppStore } from "../store";
import type { AppSettings, Hotword, RecordItem } from "../types";
import { isBubbleWindow } from "../windowKind";

export function useSharedBackendState() {
  useEffect(() => {
    if (!isTauri()) return;
    const isBubble = isBubbleWindow();
    let disposed = false;
    const store = useAppStore;
    const listeners = Promise.all([
      onEvent<{ key: string; value: unknown }>("settings-changed", ({ key, value }) => {
        if (key === "appSettings") store.getState().replaceSettings(value as AppSettings);
      }),
      onEvent<Hotword[]>("hotwords-changed", (items) => store.getState().replaceHotwords(items)),
      onEvent<RecordItem>("record-created", (record) => {
        const records = store.getState().records.filter((item) => item.id !== record.id);
        store.getState().replaceRecords([record, ...records]);
      }),
      onEvent("records-cleared", () => store.getState().replaceRecords([])),
    ]);
    void listeners.then(async () => {
      const [saved, hotwords, records] = await Promise.all([
        getSettings(), listHotwords(), listRecords(),
      ]);
      if (disposed) return;
      const persisted = saved.appSettings as AppSettings | undefined;
      if (persisted && Object.keys(persisted).length) {
        store.getState().replaceSettings(persisted);
      } else if (!isBubble) {
        let migrated = store.getState().settings;
        try {
          const legacy = JSON.parse(localStorage.getItem("yuliu-ui-v1") || "null")?.state?.settings;
          if (legacy) migrated = { ...migrated, ...legacy, llm: { ...migrated.llm, ...legacy.llm } };
        } catch { /* Invalid preview-era data is intentionally discarded. */ }
        store.getState().replaceSettings(migrated);
        await saveSettings(migrated);
      }
      if (!isBubble) localStorage.removeItem("yuliu-ui-v1");
      store.getState().replaceHotwords(hotwords);
      store.getState().replaceRecords(records);
    });
    return () => {
      disposed = true;
      void listeners.then((unlisten) => unlisten.forEach((fn) => fn()));
    };
  }, []);
}
