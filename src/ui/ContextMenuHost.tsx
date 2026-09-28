import { useEffect, useState, type ReactNode } from "react";
import { createPortal } from "react-dom";
import {
  closeAppContextMenu,
  handleAppContextMenu,
  installNativeContextMenuBlock,
  subscribeAppContextMenu,
  type AppContextMenuItem,
  type AppContextMenuState,
} from "./contextMenu";

export function ContextMenuHost({
  children,
  className,
  swallowOnly = false,
}: {
  children: ReactNode;
  className: string;
  swallowOnly?: boolean;
}) {
  const [menu, setMenu] = useState<AppContextMenuState | null>(null);

  useEffect(() => installNativeContextMenuBlock({ swallowOnly }), [swallowOnly]);

  useEffect(() => subscribeAppContextMenu(setMenu), []);

  useEffect(() => {
    if (!menu) return;
    const close = () => closeAppContextMenu();
    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Escape") close();
    };
    window.addEventListener("pointerdown", close, true);
    window.addEventListener("blur", close);
    window.addEventListener("keydown", onKey);
    window.addEventListener("resize", close);
    window.addEventListener("scroll", close, true);
    return () => {
      window.removeEventListener("pointerdown", close, true);
      window.removeEventListener("blur", close);
      window.removeEventListener("keydown", onKey);
      window.removeEventListener("resize", close);
      window.removeEventListener("scroll", close, true);
    };
  }, [menu]);

  return (
    <div
      className={className}
      onContextMenuCapture={(event) => {
        if (swallowOnly) {
          event.preventDefault();
          closeAppContextMenu();
          return;
        }
        handleAppContextMenu(event);
      }}
    >
      {children}
      {menu &&
        createPortal(
          <div
            className="app-context-menu"
            role="menu"
            style={{ left: menu.x, top: menu.y }}
            onPointerDown={(event) => event.stopPropagation()}
          >
            {menu.items.map((item: AppContextMenuItem) => (
              <button
                key={item.id}
                role="menuitem"
                className={item.danger ? "danger" : undefined}
                disabled={item.disabled}
                onClick={() => {
                  closeAppContextMenu();
                  if (!item.disabled) void item.run();
                }}
              >
                {item.label}
              </button>
            ))}
          </div>,
          document.body,
        )}
    </div>
  );
}
