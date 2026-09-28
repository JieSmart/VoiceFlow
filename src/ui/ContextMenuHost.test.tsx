import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import { closeAppContextMenu } from "./contextMenu";
import { ContextMenuHost } from "./ContextMenuHost";

describe("ContextMenuHost", () => {
  afterEach(() => {
    closeAppContextMenu();
    cleanup();
  });

  it("blocks the native menu on chrome and does not open a custom menu", () => {
    const { container } = render(
      <ContextMenuHost className="app-shell">
        <div className="titlebar">语流</div>
      </ContextMenuHost>,
    );
    const allowed = fireEvent.contextMenu(container.querySelector(".titlebar")!);
    expect(allowed).toBe(false);
    expect(screen.queryByRole("menu")).not.toBeInTheDocument();
  });

  it("opens cut/copy/paste/select all on a text field", () => {
    const { container } = render(
      <ContextMenuHost className="app-shell">
        <input defaultValue="语流" />
      </ContextMenuHost>,
    );
    const input = container.querySelector("input")!;
    input.setSelectionRange(0, 2);
    const allowed = fireEvent.contextMenu(input);
    expect(allowed).toBe(false);
    expect(screen.getByRole("menu")).toBeInTheDocument();
    expect(screen.getByRole("menuitem", { name: "剪切" })).toBeInTheDocument();
    expect(screen.getByRole("menuitem", { name: "复制" })).toBeInTheDocument();
    expect(screen.getByRole("menuitem", { name: "粘贴" })).toBeInTheDocument();
    expect(screen.getByRole("menuitem", { name: "全选" })).toBeInTheDocument();
  });

  it("does not let ctrl+a select chrome text", () => {
    const { container } = render(
      <ContextMenuHost className="app-shell">
        <h1>输入记录</h1>
        <p>全部语音转写与热词响应日志</p>
      </ContextMenuHost>,
    );
    const heading = container.querySelector("h1")!;
    fireEvent.keyDown(heading, { key: "a", ctrlKey: true });
    expect(window.getSelection()?.toString()).not.toContain("全部语音转写");
  });

  it("limits ctrl+a to the current selectable record text", () => {
    const { container } = render(
      <ContextMenuHost className="app-shell">
        <h1>输入记录</h1>
        <article>
          <div className="record-text">
            <p>为什么还有右键可以系统的那个右键出现？</p>
          </div>
        </article>
        <article>
          <div className="record-text">
            <p>出现这个是什么意思？</p>
          </div>
        </article>
      </ContextMenuHost>,
    );
    const first = container.querySelectorAll(".record-text p")[0] as HTMLElement;
    fireEvent.keyDown(first, { key: "a", ctrlKey: true });
    const selected = window.getSelection()?.toString() || "";
    expect(selected).toContain("为什么还有右键");
    expect(selected).not.toContain("出现这个是什么意思");
    expect(selected).not.toContain("输入记录");
  });

  it("swallows bubble right clicks without a custom menu", () => {
    const { container } = render(
      <ContextMenuHost className="bubble-window" swallowOnly>
        <div className="voice-bubble">录音</div>
      </ContextMenuHost>,
    );
    const allowed = fireEvent.contextMenu(container.querySelector(".voice-bubble")!);
    expect(allowed).toBe(false);
    expect(screen.queryByRole("menu")).not.toBeInTheDocument();
  });
});
