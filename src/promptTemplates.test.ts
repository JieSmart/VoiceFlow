import { describe, expect, it } from "vitest";
import {
  FALLBACK_PROMPT,
  interpolatePrompt,
  promptRequest,
  resolvePromptTemplate,
} from "./promptTemplates";

const templates = [
  {
    id: 1,
    name: "会议纪要",
    triggerWord: "纪要",
    prompt: "整理成纪要：{{text}}",
    enabled: true,
    isDefault: true,
  },
  {
    id: 2,
    name: "邮件",
    triggerWord: "会议纪要邮件",
    prompt: "写成邮件",
    enabled: true,
    isDefault: false,
  },
  {
    id: 3,
    name: "停用",
    triggerWord: "纪要",
    prompt: "停用模板",
    enabled: false,
    isDefault: false,
  },
];

describe("prompt templates", () => {
  it("replaces template variables", () => {
    expect(
      interpolatePrompt("{{datetime}} · {{hotwords}} · {{text}}", {
        text: "原文",
        hotwords: ["语流"],
        datetime: "2026-09-11",
      }),
    ).toBe("2026-09-11 · 语流 · 原文");
  });

  it("picks the longest matching trigger word", () => {
    const selected = resolvePromptTemplate(templates, "请整理会议纪要邮件", 1);
    expect(selected?.id).toBe(2);
  });

  it("falls back to the default template and hardcoded prompt", () => {
    expect(resolvePromptTemplate(templates, "普通口述", 1)?.id).toBe(1);
    expect(promptRequest(null, "原文", []).instructions).toBe(FALLBACK_PROMPT);
    expect(promptRequest(templates[1], "原文", []).instructions).toBe("写成邮件");
    expect(promptRequest(templates[1], "原文", []).input).toBe("原文");
  });
});
