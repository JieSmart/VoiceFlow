import { useState } from "react";
import {
  Check,
  ListMagnifyingGlass,
  MagnifyingGlass,
  NotePencil,
  Plus,
  Trash,
  X,
} from "@phosphor-icons/react";
import { useAppStore } from "../store";
import type { Hotword } from "../types";
import { contextMenuProps, copyText } from "../ui/contextMenu";
import { Toggle } from "../ui/primitives";

const catLabels = {
  all: "全部",
  replace: "替换词",
  cmd: "其他",
  industry: "行业词",
  contact: "联系人",
  brand: "品牌词",
};
function HotwordModal({
  item,
  onClose,
}: {
  item?: Hotword;
  onClose: () => void;
}) {
  const upsert = useAppStore((s) => s.upsertHotword);
  const [word, setWord] = useState(item?.word || "");
  const [category, setCategory] = useState<Hotword["category"]>(
    item?.category || "replace",
  );
  const [replacement, setReplacement] = useState(item?.replacement || "");
  const [priority, setPriority] = useState(
    item?.priority || useAppStore.getState().settings.defaultHotwordPriority || 3,
  );
  const [apps, setApps] = useState((item?.apps || ["全部应用"]).join("、"));
  const [note, setNote] = useState(item?.note || "");
  return (
    <div className="backdrop">
      <div className="modal compact">
        <div className="modal-head">
          <b>{item ? "编辑热词" : "新增热词"}</b>
          <button className="icon-button" onClick={onClose}>
            <X />
          </button>
        </div>
        <div className="modal-body">
          <label>
            热词
            <input
              value={word}
              onChange={(e) => setWord(e.target.value)}
              placeholder="例如：双周会"
            />
          </label>
          <div className="form-grid">
            <label>
              类别
              <select
                value={category}
                onChange={(e) =>
                  setCategory(e.target.value as Hotword["category"])
                }
              >
                {Object.entries(catLabels)
                  .slice(1)
                  .map(([v, l]) => (
                    <option value={v} key={v}>
                      {l}
                    </option>
                  ))}
              </select>
            </label>
            <label>
              优先级
              <input
                type="range"
                min="1"
                max="5"
                value={priority}
                onChange={(e) => setPriority(+e.target.value)}
              />
              <span>{priority}</span>
            </label>
          </div>
          <label>
            替换结果
            <input
              value={replacement}
              onChange={(e) => setReplacement(e.target.value)}
              placeholder="留空则仅统计命中，不改写"
            />
          </label>
          <small className="field-hint">
            类别只用于筛选。有替换结果则做识别后子串替换，留空则仅统计。
          </small>
          <label>
            生效应用
            <input
              value={apps}
              onChange={(e) => setApps(e.target.value)}
              placeholder="全部应用，或用顿号/逗号分隔窗口标题关键词"
            />
          </label>
          <small className="field-hint">
            按录音开始时前台窗口标题包含匹配，不是进程名。
          </small>
          <label>
            备注
            <input value={note} onChange={(e) => setNote(e.target.value)} placeholder="可选" />
          </label>
        </div>
        <div className="modal-foot">
          <button className="button" onClick={onClose}>
            取消
          </button>
          <button
            className="button primary"
            disabled={!word.trim()}
            onClick={() => {
              upsert({
                id: item?.id || Date.now(),
                word: word.trim(),
                category,
                behavior: replacement
                  ? `替换为「${replacement}」`
                  : "仅统计，不强改写",
                replacement,
                priority,
                apps: apps.split(/[，,、]/).map((value) => value.trim()).filter(Boolean),
                hits: item?.hits || 0,
                lastUsed: item?.lastUsed || "从未",
                enabled: item?.enabled ?? true,
                note,
              });
              onClose();
            }}
          >
            <Check />
            保存热词
          </button>
        </div>
      </div>
    </div>
  );
}

export function HotwordsPage() {
  const all = useAppStore((s) => s.hotwords),
    del = useAppStore((s) => s.deleteHotword),
    upsert = useAppStore((s) => s.upsertHotword);
  const [cat, setCat] = useState("all"),
    [q, setQ] = useState(""),
    [editing, setEditing] = useState<Hotword | "new" | null>(null);
  const rows = all.filter(
    (h) =>
      (cat === "all" || h.category === cat) &&
      h.word.toLowerCase().includes(q.toLowerCase()),
  );
  return (
    <div className="content">
      <div className="toolbar">
        <div className="segment categories">
          {Object.entries(catLabels).map(([v, l]) => (
            <button
              className={cat === v ? "active" : ""}
              onClick={() => setCat(v)}
              key={v}
            >
              {l}
            </button>
          ))}
        </div>
        <div className="search">
          <MagnifyingGlass />
          <input
            placeholder="搜索热词…"
            value={q}
            onChange={(e) => setQ(e.target.value)}
          />
        </div>
        <button className="button primary" onClick={() => setEditing("new")}>
          <Plus />
          新增热词
        </button>
      </div>
      <div className="card table-card">
        <table>
          <thead>
            <tr>
              <th>热词</th>
              <th>类型 / 行为</th>
              <th>优先级</th>
              <th>生效应用</th>
              <th>命中</th>
              <th>最后使用</th>
              <th>状态</th>
              <th />
            </tr>
          </thead>
          <tbody>
            {rows.map((h) => (
              <tr
                key={h.id}
                {...contextMenuProps(`hotword-${h.id}`, [
                  {
                    id: "copy-word",
                    label: "复制热词",
                    run: () => void copyText(h.word),
                  },
                  {
                    id: "edit",
                    label: "编辑",
                    run: () => setEditing(h),
                  },
                  {
                    id: "delete",
                    label: "删除",
                    danger: true,
                    run: () => del(h.id),
                  },
                ])}
              >
                <td>
                  <b>{h.word}</b>
                  {h.note && <small>{h.note}</small>}
                </td>
                <td>
                  <span className={`category ${h.category}`}>
                    {catLabels[h.category]}
                  </span>
                  <small>{h.behavior}</small>
                </td>
                <td>
                  <div className="priority">
                    {[1, 2, 3, 4, 5].map((i) => (
                      <i className={i <= h.priority ? "on" : ""} key={i} />
                    ))}
                  </div>
                </td>
                <td>{h.apps.join(" · ")}</td>
                <td className="mono">{h.hits}</td>
                <td>{h.lastUsed}</td>
                <td>
                  <Toggle
                    on={h.enabled}
                    onChange={(enabled) => upsert({ ...h, enabled })}
                  />
                </td>
                <td>
                  <div className="row-actions">
                    <button onClick={() => setEditing(h)}>
                      <NotePencil />
                    </button>
                    <button onClick={() => del(h.id)}>
                      <Trash />
                    </button>
                  </div>
                </td>
              </tr>
            ))}
          </tbody>
        </table>
        {!rows.length && <div className="empty">没有符合条件的热词</div>}
      </div>
      {editing && (
        <HotwordModal
          item={editing === "new" ? undefined : editing}
          onClose={() => setEditing(null)}
        />
      )}
    </div>
  );
}
