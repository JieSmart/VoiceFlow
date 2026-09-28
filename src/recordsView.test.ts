import { describe, expect, it } from "vitest";
import { filterRecords, recentHotwordRecords } from "./recordsView";
import type { RecordItem } from "./types";

const records: RecordItem[] = [
  {
    id: "1",
    startedAt: "2026-09-11T01:00:00",
    app: "飞书",
    duration: 8,
    chars: 12,
    hotwords: ["语流"],
    ai: true,
    text: "语流纪要",
    formattedText: "整理后的纪要",
  },
  {
    id: "2",
    startedAt: "2026-09-11T02:00:00",
    app: "Chrome",
    duration: 3,
    chars: 4,
    hotwords: [],
    ai: false,
    text: "打开浏览器",
  },
];

describe("records view", () => {
  it("filters by hotword, AI, app and query", () => {
    expect(filterRecords(records, { filter: "hotword" }).map((item) => item.id)).toEqual(["1"]);
    expect(filterRecords(records, { filter: "ai" }).map((item) => item.id)).toEqual(["1"]);
    expect(filterRecords(records, { app: "Chrome" }).map((item) => item.id)).toEqual(["2"]);
    expect(filterRecords(records, { query: "整理后" }).map((item) => item.id)).toEqual(["1"]);
  });

  it("returns recent records that actually hit hotwords", () => {
    expect(recentHotwordRecords(records).map((item) => item.id)).toEqual(["1"]);
  });
});
