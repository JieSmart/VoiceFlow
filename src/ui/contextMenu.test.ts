import { afterEach, describe, expect, it, vi } from "vitest";
import {
  clampMenuPosition,
  closeAppContextMenu,
  confineDocumentSelectAll,
  contextMenuProps,
  copyMenuItems,
  fieldMenuItems,
  handleAppContextMenu,
  installNativeContextMenuBlock,
  isDocumentSelectAllShortcut,
  isEditableTarget,
  nearestBoundMenuItems,
  subscribeAppContextMenu,
} from "./contextMenu";

describe("contextMenu", () => {
  afterEach(() => {
    closeAppContextMenu();
  });

  it("clamps the menu inside the viewport", () => {
    expect(clampMenuPosition(2000, 2000, 4, { width: 400, height: 300 })).toEqual({
      x: 216,
      y: 144,
    });
    expect(clampMenuPosition(-20, -20, 2, { width: 400, height: 300 })).toEqual({
      x: 8,
      y: 8,
    });
  });

  it("treats text inputs as editable and skips range controls", () => {
    const text = document.createElement("input");
    text.type = "text";
    const range = document.createElement("input");
    range.type = "range";
    const area = document.createElement("textarea");
    expect(isEditableTarget(text)).toBe(true);
    expect(isEditableTarget(range)).toBe(false);
    expect(isEditableTarget(area)).toBe(true);
  });

  it("disables copy and cut for password fields", () => {
    const secret = document.createElement("input");
    secret.type = "password";
    secret.value = "hidden";
    const items = fieldMenuItems(secret);
    expect(items.find((item) => item.id === "copy")?.disabled).toBe(true);
    expect(items.find((item) => item.id === "cut")?.disabled).toBe(true);
    expect(items.find((item) => item.id === "paste")?.disabled).toBeFalsy();
  });

  it("treats ctrl/cmd+a as a document select-all shortcut", () => {
    expect(isDocumentSelectAllShortcut({ key: "a", ctrlKey: true, metaKey: false, altKey: false })).toBe(true);
    expect(isDocumentSelectAllShortcut({ key: "A", ctrlKey: false, metaKey: true, altKey: false })).toBe(true);
    expect(isDocumentSelectAllShortcut({ key: "a", ctrlKey: true, metaKey: false, altKey: true })).toBe(false);
    expect(isDocumentSelectAllShortcut({ key: "c", ctrlKey: true, metaKey: false, altKey: false })).toBe(false);
  });

  it("blocks page-wide select all outside editable fields", () => {
    const heading = document.createElement("h1");
    heading.textContent = "输入记录";
    document.body.appendChild(heading);
    const event = {
      key: "a",
      ctrlKey: true,
      metaKey: false,
      altKey: false,
      target: heading,
      preventDefault: vi.fn(),
    } as unknown as KeyboardEvent;
    expect(confineDocumentSelectAll(event)).toBe(true);
    expect(event.preventDefault).toHaveBeenCalled();
    heading.remove();
  });

  it("lets select all through on a focused text field", () => {
    const input = document.createElement("input");
    input.value = "语流";
    document.body.appendChild(input);
    const event = {
      key: "a",
      ctrlKey: true,
      metaKey: false,
      altKey: false,
      target: input,
      preventDefault: vi.fn(),
    } as unknown as KeyboardEvent;
    expect(confineDocumentSelectAll(event)).toBe(false);
    expect(event.preventDefault).not.toHaveBeenCalled();
    input.remove();
  });

  it("reads bound menu items from a nested target", () => {
    const row = document.createElement("article");
    const text = document.createElement("p");
    const props = contextMenuProps(
      "record-nested",
      copyMenuItems([{ id: "copy-original", label: "复制原文", text: "原文内容" }]),
    );
    row.setAttribute("data-context-menu", props["data-context-menu"]);
    row.appendChild(text);
    expect(nearestBoundMenuItems(text).map((item) => item.label)).toEqual(["复制原文"]);
  });

  it("opens bound menus from the capture-phase listener", () => {
    const stopInstall = installNativeContextMenuBlock();
    const seen: string[][] = [];
    const stop = subscribeAppContextMenu((menu) => {
      seen.push(menu?.items.map((item) => item.label) ?? []);
    });
    const row = document.createElement("article");
    const props = contextMenuProps(
      "record-capture",
      copyMenuItems([{ id: "copy-original", label: "复制原文", text: "原文内容" }]),
    );
    row.setAttribute("data-context-menu", props["data-context-menu"]);
    document.body.appendChild(row);
    const event = new MouseEvent("contextmenu", { bubbles: true, cancelable: true, clientX: 12, clientY: 24 });
    expect(row.dispatchEvent(event)).toBe(false);
    expect(seen.at(-1)).toEqual(["复制原文"]);
    row.remove();
    stop();
    stopInstall();
  });

  it("opens a custom menu and still prevents the native event", () => {
    const seen: string[][] = [];
    const stop = subscribeAppContextMenu((menu) => {
      seen.push(menu?.items.map((item) => item.label) ?? []);
    });
    const event = {
      preventDefault: vi.fn(),
      stopPropagation: vi.fn(),
      clientX: 40,
      clientY: 50,
      target: document.createElement("div"),
    };
    handleAppContextMenu(event, copyMenuItems([{ id: "copy-path", label: "复制路径", text: "D:\\语流" }]));
    expect(event.preventDefault).toHaveBeenCalled();
    expect(seen.at(-1)).toEqual(["复制路径"]);
    stop();
  });
});
