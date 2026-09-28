import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { closeAppContextMenu } from "../ui/contextMenu";
import { ContextMenuHost } from "../ui/ContextMenuHost";
import { defaultSettings, useAppStore } from "../store";
import { RecordsPage } from "./RecordsPage";

describe("RecordsPage context menu", () => {
  beforeEach(() => {
    useAppStore.setState({
      route: "records",
      settings: defaultSettings,
      records: [
        {
          id: "r1",
          startedAt: "2026-09-15T08:00:00Z",
          app: "记事本",
          duration: 4,
          chars: 8,
          hotwords: [],
          ai: true,
          text: "原文内容",
          formattedText: "格式化内容",
        },
      ],
    });
  });

  afterEach(() => {
    closeAppContextMenu();
    cleanup();
  });

  it("opens copy actions on a record row and blocks the native menu", () => {
    const { container } = render(
      <ContextMenuHost className="app-shell">
        <RecordsPage />
      </ContextMenuHost>,
    );
    const row = container.querySelector("article")!;
    expect(row.getAttribute("data-context-menu")).toBe("record-r1");
    expect(fireEvent.contextMenu(row)).toBe(false);
    expect(screen.getByRole("menuitem", { name: "复制原文" })).toBeInTheDocument();
    expect(screen.getByRole("menuitem", { name: "复制格式化结果" })).toBeInTheDocument();
    act(() => {
      useAppStore.getState().setRoute("home");
    });
    expect(screen.queryByRole("menu")).not.toBeInTheDocument();
  });
});
