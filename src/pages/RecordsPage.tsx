import { useState } from "react";
import { Copy, MagnifyingGlass, Sparkle, Tag } from "@phosphor-icons/react";
import { filterRecords, recordApps, type RecordFilter } from "../recordsView";
import { useAppStore } from "../store";
import { contextMenuProps, copyMenuItems, copyText } from "../ui/contextMenu";

export function RecordsPage() {
  const records = useAppStore((s) => s.records);
  const [q, setQ] = useState(""),
    [filter, setFilter] = useState<RecordFilter>("all"),
    [app, setApp] = useState("全部"),
    [open, setOpen] = useState<string | null>(null);
  const apps = recordApps(records);
  const list = filterRecords(records, { query: q, filter, app });
  return (
    <div className="content">
      <div className="toolbar">
        <div className="segment">
          {([
            ["all", "全部输入"],
            ["hotword", "有热词"],
            ["ai", "已 AI"],
          ] as const).map(([value, label]) => (
            <button
              key={value}
              className={filter === value ? "active" : ""}
              onClick={() => setFilter(value)}
            >
              {label}
            </button>
          ))}
        </div>
        <select value={app} onChange={(e) => setApp(e.target.value)}>
          {apps.map((item) => (
            <option value={item} key={item}>
              {item}
            </option>
          ))}
        </select>
        <div className="search push">
          <MagnifyingGlass />
          <input
            placeholder="搜索内容…"
            value={q}
            onChange={(e) => setQ(e.target.value)}
          />
        </div>
      </div>
      <div className="card records">
        {list.map((r) => (
          <article
            className={open === r.id ? "open" : ""}
            key={r.id}
            onClick={() => setOpen(open === r.id ? null : r.id)}
            {...contextMenuProps(
              `record-${r.id}`,
              copyMenuItems([
                { id: "copy-original", label: "复制原文", text: r.text },
                {
                  id: "copy-formatted",
                  label: "复制格式化结果",
                  text: r.formattedText || "",
                },
              ]),
            )}
          >
            <div className="record-time">
              <b>{new Date(r.startedAt).toLocaleTimeString("zh-CN", { hour: "2-digit", minute: "2-digit" })}</b>
              <small>{new Date(r.startedAt).toLocaleDateString("zh-CN")}</small>
            </div>
            <div className="record-app">
              <b>{r.app}</b>
              <small>
                {r.duration} 秒 · {r.chars} 字
              </small>
            </div>
            <div className="record-text">
              <p>{r.formattedText || r.text}</p>
              <div>
                {r.hotwords.map((x) => (
                  <span className="chip" key={x}>
                    <Tag />
                    {x}
                  </span>
                ))}
                {r.ai && (
                  <span className="chip ai">
                    <Sparkle />
                    AI
                  </span>
                )}
              </div>
              {open === r.id && (
                <div className="record-detail">
                  <span>
                    {r.model || "未使用 AI 格式化"}
                    {r.ai && r.llmProvider && ` · ${r.llmProvider === "openai" ? "OpenAI 格式" : "Claude 格式"}`}
                  </span>
                  <button
                    className="button small"
                    onClick={(e) => {
                      e.stopPropagation();
                      void copyText(r.text);
                    }}
                  >
                    <Copy />
                    复制原文
                  </button>
                  {r.formattedText && (
                    <button
                      className="button small"
                      onClick={(e) => {
                        e.stopPropagation();
                        void copyText(r.formattedText || "");
                      }}
                    >
                      <Copy />复制格式化结果
                    </button>
                  )}
                </div>
              )}
            </div>
            <span className="record-status">
              <i />
              已上屏
            </span>
          </article>
        ))}
        {!list.length && <div className="empty">还没有真实的语音输入记录</div>}
      </div>
    </div>
  );
}
