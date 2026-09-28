import { ClockCounterClockwise, GearSix, House, Tag } from "@phosphor-icons/react";
import clsx from "clsx";
import { acceleratorParts } from "../hotkeys";
import { useAppStore } from "../store";
import { Kbd, WaveLogo } from "../ui/primitives";

export function Sidebar() {
  const route = useAppStore((s) => s.route),
    setRoute = useAppStore((s) => s.setRoute),
    open = useAppStore((s) => s.openSettings),
    count = useAppStore((s) => s.hotwords.length),
    wake = useAppStore((s) => s.settings.hotkeys.wake);
  const items = [
    { id: "home" as const, label: "主页", icon: House },
    { id: "hotwords" as const, label: "热词库", icon: Tag, badge: count },
    { id: "records" as const, label: "输入记录", icon: ClockCounterClockwise },
  ];
  return (
    <aside className="sidebar">
      <div className="brand">
        <WaveLogo />
        <div>
          <strong>语流</strong>
          <small>说了就上屏</small>
        </div>
        <em>v0.1.0</em>
      </div>
      <div className="side-label">工作台</div>
      <nav>
        {items.map(({ id, label, icon: Icon, badge }) => (
          <button
            key={id}
            className={clsx("nav-item", route === id && "active")}
            onClick={() => setRoute(id)}
          >
            <Icon size={16} />
            <span>{label}</span>
            {badge && <b>{badge}</b>}
          </button>
        ))}
      </nav>
      <div className="side-spacer" />
      <div className="wake-row">
        <span>唤醒键</span>
        {acceleratorParts(wake).map((part) => (
          <Kbd key={part}>{part}</Kbd>
        ))}
        <button onClick={() => open("hotkey")}>更改</button>
      </div>
      <button className="settings-entry" onClick={() => open()}>
        <GearSix size={17} />
        <span>设置</span>
      </button>
    </aside>
  );
}
