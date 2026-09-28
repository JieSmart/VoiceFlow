export type AppContextMenuItem = {
  id: string;
  label: string;
  disabled?: boolean;
  danger?: boolean;
  run: () => void | Promise<void>;
};

export type AppContextMenuState = {
  x: number;
  y: number;
  items: AppContextMenuItem[];
};

const MENU_WIDTH = 176;
const MENU_ITEM_HEIGHT = 34;
const MENU_PADDING = 12;

type MenuListener = (menu: AppContextMenuState | null) => void;

const listeners = new Set<MenuListener>();
let currentMenu: AppContextMenuState | null = null;

function emit(menu: AppContextMenuState | null) {
  currentMenu = menu;
  for (const listener of listeners) listener(menu);
}

export function subscribeAppContextMenu(listener: MenuListener) {
  listeners.add(listener);
  listener(currentMenu);
  return () => {
    listeners.delete(listener);
  };
}

export function closeAppContextMenu() {
  emit(null);
}

export function openAppContextMenu(clientX: number, clientY: number, items: AppContextMenuItem[]) {
  if (!items.length) {
    emit(null);
    return;
  }
  emit({
    ...clampMenuPosition(clientX, clientY, items.length),
    items,
  });
}

export function isEditableTarget(target: EventTarget | null): target is HTMLInputElement | HTMLTextAreaElement {
  if (!(target instanceof HTMLElement)) return false;
  if (target instanceof HTMLTextAreaElement) return !target.disabled && !target.readOnly;
  if (target instanceof HTMLInputElement) {
    if (target.disabled || target.readOnly) return false;
    const type = (target.type || "text").toLowerCase();
    return !["button", "checkbox", "color", "file", "hidden", "image", "radio", "range", "reset", "submit"].includes(type);
  }
  return false;
}

const SELECTABLE_SELECTOR = [
  "input:not([type='button']):not([type='checkbox']):not([type='radio']):not([type='range']):not([type='file']):not([type='submit']):not([type='reset']):not([type='hidden'])",
  "textarea",
  ".selectable-text",
  ".record-text p",
  ".prompt-card > small",
  ".storage-path-block code",
  ".storage-confirm code",
  ".model-directory code",
  "td > b",
  "td > small",
].join(",");

export function isDocumentSelectAllShortcut(event: { key: string; ctrlKey: boolean; metaKey: boolean; altKey: boolean }) {
  return (event.ctrlKey || event.metaKey) && !event.altKey && event.key.toLowerCase() === "a";
}

export function nearestSelectableElement(target: EventTarget | null): HTMLElement | null {
  if (!(target instanceof Node)) return document.activeElement instanceof HTMLElement ? document.activeElement : null;
  const fromNode = target instanceof Element ? target : target.parentElement;
  return fromNode?.closest(SELECTABLE_SELECTOR) as HTMLElement | null;
}

export function selectInside(element: HTMLElement) {
  if (isEditableTarget(element)) {
    element.focus();
    element.setSelectionRange(0, element.value.length);
    return;
  }
  const selection = window.getSelection();
  if (!selection) return;
  const range = document.createRange();
  range.selectNodeContents(element);
  selection.removeAllRanges();
  selection.addRange(range);
}

export function confineDocumentSelectAll(event: KeyboardEvent) {
  if (!isDocumentSelectAllShortcut(event)) return false;
  const target = nearestSelectableElement(event.target) ?? nearestSelectableElement(document.activeElement);
  if (isEditableTarget(target)) return false;
  event.preventDefault();
  if (target) selectInside(target);
  return true;
}

export function clampMenuPosition(
  clientX: number,
  clientY: number,
  itemCount: number,
  viewport = { width: typeof window === "undefined" ? 1280 : window.innerWidth, height: typeof window === "undefined" ? 760 : window.innerHeight },
) {
  const width = MENU_WIDTH;
  const height = MENU_PADDING + itemCount * MENU_ITEM_HEIGHT;
  return {
    x: Math.max(8, Math.min(clientX, viewport.width - width - 8)),
    y: Math.max(8, Math.min(clientY, viewport.height - height - 8)),
  };
}

export function copyText(text: string) {
  if (!text) return Promise.resolve();
  return navigator.clipboard.writeText(text);
}

function currentRange(field: HTMLInputElement | HTMLTextAreaElement) {
  const start = field.selectionStart ?? 0;
  const end = field.selectionEnd ?? 0;
  return { start, end, selected: field.value.slice(start, end) };
}

function replaceRange(field: HTMLInputElement | HTMLTextAreaElement, next: string, caret: number) {
  const prototype = field instanceof HTMLTextAreaElement
    ? HTMLTextAreaElement.prototype
    : HTMLInputElement.prototype;
  const descriptor = Object.getOwnPropertyDescriptor(prototype, "value");
  descriptor?.set?.call(field, next);
  field.dispatchEvent(new Event("input", { bubbles: true }));
  field.setSelectionRange(caret, caret);
}

export async function runFieldCommand(
  field: HTMLInputElement | HTMLTextAreaElement,
  command: "cut" | "copy" | "paste" | "selectAll",
) {
  field.focus();
  const { start, end, selected } = currentRange(field);
  if (command === "selectAll") {
    field.setSelectionRange(0, field.value.length);
    return;
  }
  if (command === "copy") {
    if (!selected) return;
    await copyText(selected);
    return;
  }
  if (command === "cut") {
    if (!selected) return;
    await copyText(selected);
    const next = field.value.slice(0, start) + field.value.slice(end);
    replaceRange(field, next, start);
    return;
  }
  const pasted = await navigator.clipboard.readText();
  const next = field.value.slice(0, start) + pasted + field.value.slice(end);
  replaceRange(field, next, start + pasted.length);
}

export function fieldMenuItems(field: HTMLInputElement | HTMLTextAreaElement): AppContextMenuItem[] {
  const selected = Boolean(field.selectionStart !== field.selectionEnd);
  const secret = field instanceof HTMLInputElement && field.type === "password";
  return [
    {
      id: "cut",
      label: "剪切",
      disabled: !selected || secret,
      run: () => void runFieldCommand(field, "cut"),
    },
    {
      id: "copy",
      label: "复制",
      disabled: !selected || secret,
      run: () => void runFieldCommand(field, "copy"),
    },
    {
      id: "paste",
      label: "粘贴",
      run: () => void runFieldCommand(field, "paste"),
    },
    {
      id: "selectAll",
      label: "全选",
      disabled: !field.value,
      run: () => void runFieldCommand(field, "selectAll"),
    },
  ];
}

export function copyMenuItems(entries: { id: string; label: string; text: string }[]): AppContextMenuItem[] {
  return entries
    .filter((entry) => entry.text)
    .map((entry) => ({
      id: entry.id,
      label: entry.label,
      run: () => void copyText(entry.text),
    }));
}

const MENU_HOST_ATTR = "data-context-menu";
const menuRegistry = new Map<string, AppContextMenuItem[]>();

export function contextMenuProps(id: string, items: AppContextMenuItem[]) {
  menuRegistry.set(id, items);
  return {
    [MENU_HOST_ATTR]: id,
  } as Record<string, string>;
}

export function nearestBoundMenuItems(target: EventTarget | null): AppContextMenuItem[] {
  if (!(target instanceof Element)) {
    return [];
  }
  const host = target.closest(`[${MENU_HOST_ATTR}]`);
  const id = host?.getAttribute(MENU_HOST_ATTR);
  return (id && menuRegistry.get(id)) || [];
}

export function handleAppContextMenu(
  event: { preventDefault(): void; clientX: number; clientY: number; target: EventTarget | null },
  extra: AppContextMenuItem[] = [],
) {
  event.preventDefault();
  const bound = extra.length ? extra : nearestBoundMenuItems(event.target);
  const fieldItems = isEditableTarget(event.target) ? fieldMenuItems(event.target) : [];
  const items = bound.length ? bound : fieldItems;
  openAppContextMenu(event.clientX, event.clientY, items);
}

export function installNativeContextMenuBlock(options?: { swallowOnly?: boolean }) {
  const blockMenu = (event: MouseEvent) => {
    event.preventDefault();
    if (options?.swallowOnly) {
      closeAppContextMenu();
      return;
    }
    handleAppContextMenu(event);
  };
  const blockSelectAll = (event: KeyboardEvent) => {
    confineDocumentSelectAll(event);
  };
  document.addEventListener("contextmenu", blockMenu, true);
  window.addEventListener("contextmenu", blockMenu, true);
  document.addEventListener("keydown", blockSelectAll, true);
  return () => {
    document.removeEventListener("contextmenu", blockMenu, true);
    window.removeEventListener("contextmenu", blockMenu, true);
    document.removeEventListener("keydown", blockSelectAll, true);
  };
}
