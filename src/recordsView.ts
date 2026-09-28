import type { RecordItem } from "./types";

export type RecordFilter = "all" | "hotword" | "ai";
export type UsageRange = "day" | "week" | "month";

export function filterRecords(
  records: RecordItem[],
  options: { query?: string; filter?: RecordFilter; app?: string },
) {
  const query = (options.query || "").trim();
  const filter = options.filter || "all";
  const app = options.app && options.app !== "全部" ? options.app : "";
  return records.filter((record) => {
    if (filter === "hotword" && !record.hotwords.length) return false;
    if (filter === "ai" && !record.ai) return false;
    if (app && record.app !== app) return false;
    if (!query) return true;
    return `${record.text}\n${record.formattedText || ""}\n${record.app}\n${record.hotwords.join(" ")}`.includes(query);
  });
}

export function recordApps(records: RecordItem[]) {
  return ["全部", ...Array.from(new Set(records.map((record) => record.app).filter(Boolean)))];
}

function localDayKey(value: string) {
  return new Date(value).toLocaleDateString("sv-SE");
}

export function usageSeries(records: RecordItem[], range: UsageRange) {
  if (range === "day") {
    const today = new Date().toLocaleDateString("sv-SE");
    const hours = Array.from({ length: 24 }, (_, hour) => ({
      label: String(hour),
      value: 0,
    }));
    for (const record of records) {
      const started = new Date(record.startedAt);
      if (started.toLocaleDateString("sv-SE") !== today) continue;
      hours[started.getHours()].value += record.chars;
    }
    return hours;
  }
  if (range === "week") {
    const days = Array.from({ length: 7 }, (_, index) => {
      const day = new Date();
      day.setDate(day.getDate() - (6 - index));
      return {
        key: day.toLocaleDateString("sv-SE"),
        label: ["日", "一", "二", "三", "四", "五", "六"][day.getDay()],
        value: 0,
      };
    });
    for (const record of records) {
      const key = localDayKey(record.startedAt);
      const item = days.find((day) => day.key === key);
      if (item) item.value += record.chars;
    }
    return days.map(({ label, value }) => ({ label, value }));
  }
  const days = Array.from({ length: 30 }, (_, index) => {
    const day = new Date();
    day.setDate(day.getDate() - (29 - index));
    return {
      key: day.toLocaleDateString("sv-SE"),
      label: String(day.getDate()),
      value: 0,
    };
  });
  for (const record of records) {
    const key = localDayKey(record.startedAt);
    const item = days.find((day) => day.key === key);
    if (item) item.value += record.chars;
  }
  return days.map(({ label, value }) => ({ label, value }));
}

export function recentHotwordRecords(records: RecordItem[], limit = 6) {
  return records.filter((record) => record.hotwords.length).slice(0, limit);
}
