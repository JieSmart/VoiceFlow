import { render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { PromptSettings } from "./settings/PromptSettings";
import type { PromptTemplate } from "./promptTemplates";

vi.mock("./api", () => ({
  listPrompts: vi.fn(),
  onEvent: vi.fn(async () => () => undefined),
  removePrompt: vi.fn(),
  savePrompt: vi.fn(),
}));

import { listPrompts } from "./api";

const templates: PromptTemplate[] = [
  {
    id: 1,
    name: "默认整理",
    triggerWord: "",
    prompt: "整理：{{text}}",
    enabled: true,
    isDefault: true,
  },
  {
    id: 2,
    name: "邮件",
    triggerWord: "邮件",
    prompt: "写成邮件",
    enabled: true,
    isDefault: false,
  },
];

describe("PromptSettings", () => {
  beforeEach(() => {
    vi.mocked(listPrompts).mockResolvedValue(templates);
  });

  it("hides delete on default templates and keeps it for custom ones", async () => {
    render(<PromptSettings />);
    expect(await screen.findAllByText("默认整理")).not.toHaveLength(0);
    const cards = document.querySelectorAll(".prompt-card");
    expect(cards[0].textContent).not.toContain("删除");
    expect(cards[1].textContent).toContain("删除");
  });
});
