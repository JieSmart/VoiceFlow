import { useEffect, useState } from "react";
import { X } from "@phosphor-icons/react";
import { listPrompts, onEvent, removePrompt, savePrompt } from "../api";
import { type PromptTemplate } from "../promptTemplates";
import { useAppStore } from "../store";
import { contextMenuProps, copyText } from "../ui/contextMenu";
import { SettingsHeader, SettingRow, Toggle } from "../ui/primitives";

export function PromptSettings() {
  const settings = useAppStore((s) => s.settings);
  const patch = useAppStore((s) => s.patchSettings);
  const [templates, setTemplates] = useState<PromptTemplate[]>([]);
  const [editing, setEditing] = useState<PromptTemplate | "new" | null>(null);
  const [name, setName] = useState("");
  const [triggerWord, setTriggerWord] = useState("");
  const [prompt, setPrompt] = useState("");
  const reload = () => void listPrompts().then(setTemplates);
  useEffect(() => {
    reload();
    const listener = onEvent<PromptTemplate[]>("prompts-changed", setTemplates);
    return () => {
      void listener.then((unlisten) => unlisten());
    };
  }, []);
  const beginEdit = (item?: PromptTemplate) => {
    setEditing(item || "new");
    setName(item?.name || "");
    setTriggerWord(item?.triggerWord || "");
    setPrompt(item?.prompt || "");
  };
  return (
    <>
      <SettingsHeader title="提示词模板" desc="决定 AI 格式化如何整理转写；说话中的触发词可临时切换" />
      <div className="settings-card">
        <SettingRow title="当前默认模板">
          <select
            value={settings.defaultPromptId || templates.find((item) => item.isDefault)?.id || ""}
            onChange={(e) => patch({ defaultPromptId: Number(e.target.value) || undefined })}
          >
            {templates.filter((item) => item.enabled).map((item) => (
              <option value={item.id} key={item.id}>
                {item.name}
              </option>
            ))}
          </select>
        </SettingRow>
        <SettingRow title="新建模板">
          <button className="button small primary" onClick={() => beginEdit()}>
            新建模板
          </button>
        </SettingRow>
      </div>
      <div className="prompt-grid">
        {templates.map((item) => (
          <div
            className="settings-card spaced prompt-card"
            key={item.id}
            {...contextMenuProps(`prompt-${item.id}`, [
              {
                id: "copy-prompt",
                label: "复制提示词",
                disabled: !item.prompt,
                run: () => void copyText(item.prompt),
              },
              {
                id: "edit",
                label: "编辑",
                run: () => beginEdit(item),
              },
              ...(!item.isDefault
                ? [{
                    id: "delete",
                    label: "删除",
                    danger: true,
                    run: () => void removePrompt(item.id).then(reload),
                  }]
                : []),
            ])}
          >
            <div className="model-manager-head">
              <div>
                <b>{item.name}</b>
                {item.isDefault && <span className="chip ai">默认</span>}
                {item.triggerWord && <span className="chip">触发词「{item.triggerWord}」</span>}
              </div>
              <Toggle
                on={item.enabled}
                onChange={(enabled) => void savePrompt({ ...item, enabled }).then(reload)}
              />
            </div>
            <small>{item.prompt || "（空白模板）"}</small>
            <div className="model-actions">
              <button className="button small" onClick={() => beginEdit(item)}>
                编辑
              </button>
              {!item.isDefault && (
                <button
                  className="button small"
                  onClick={() =>
                    void savePrompt({ ...item, isDefault: true }).then((saved) => {
                      patch({ defaultPromptId: saved.id });
                      reload();
                    })
                  }
                >
                  设为默认
                </button>
              )}
              {!item.isDefault && (
                <button className="button small" onClick={() => void removePrompt(item.id).then(reload)}>
                  删除
                </button>
              )}
            </div>
          </div>
        ))}
      </div>
      {editing && (
        <div className="backdrop">
          <div className="modal compact">
            <div className="modal-head">
              <b>{editing === "new" ? "新建提示词模板" : "编辑提示词模板"}</b>
              <button className="icon-button" onClick={() => setEditing(null)}>
                <X />
              </button>
            </div>
            <div className="modal-body">
              <label>
                模板名称
                <input value={name} onChange={(e) => setName(e.target.value)} />
              </label>
              <label>
                语音触发词（可选）
                <input value={triggerWord} onChange={(e) => setTriggerWord(e.target.value)} />
              </label>
              <label>
                System Prompt
                <textarea value={prompt} onChange={(e) => setPrompt(e.target.value)} rows={8} />
              </label>
              <small>可用变量：{"{{text}}"} {"{{hotwords}}"} {"{{datetime}}"}</small>
            </div>
            <div className="modal-foot">
              <button className="button" onClick={() => setEditing(null)}>
                取消
              </button>
              <button
                className="button primary"
                disabled={!name.trim()}
                onClick={() => {
                  const current = editing === "new" ? undefined : editing;
                  void savePrompt({
                    id: current?.id || 0,
                    name: name.trim(),
                    triggerWord: triggerWord.trim(),
                    prompt,
                    enabled: current?.enabled ?? true,
                    isDefault: current?.isDefault ?? false,
                  }).then((saved) => {
                    if (!settings.defaultPromptId) patch({ defaultPromptId: saved.id });
                    setEditing(null);
                    reload();
                  });
                }}
              >
                保存模板
              </button>
            </div>
          </div>
        </div>
      )}
    </>
  );
}
