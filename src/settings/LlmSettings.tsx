import { useEffect, useState } from "react";
import { Sparkle } from "@phosphor-icons/react";
import clsx from "clsx";
import { listLlmModels, secretConfigured, setSecret, testLlm } from "../api";
import { useAppStore } from "../store";
import type { AppError, LlmProvider } from "../types";
import { SettingsHeader, SettingRow, Toggle } from "../ui/primitives";

function LlmProviderPane({ provider }: { provider: LlmProvider }) {
  const settings = useAppStore((s) => s.settings),
    patch = useAppStore((s) => s.patchSettings);
  const cfg = settings.llm[provider];
  const [key, setKey] = useState(""),
    [busy, setBusy] = useState(false),
    [notice, setNotice] = useState(""),
    [models, setModels] = useState<Record<string, unknown>[]>([]),
    [listing, setListing] = useState(false);
  const update = (next: Partial<typeof cfg>) =>
    patch({ llm: { ...settings.llm, [provider]: { ...cfg, ...next } } });
  useEffect(() => {
    setModels([]);
    setNotice("");
  }, [provider]);
  const save = async () => {
    setBusy(true);
    setNotice("");
    try {
      if (key) await setSecret(`llm-${provider}`, key);
      await testLlm(provider, cfg.model, cfg.baseUrl);
      update({ configured: true });
      setKey("");
      setNotice("连接成功，密钥已安全保存");
    } catch (e) {
      setNotice((e as AppError)?.message || String(e));
    } finally {
      setBusy(false);
    }
  };
  const fetchModels = async () => {
    setListing(true);
    setNotice("");
    try {
      if (key) {
        await setSecret(`llm-${provider}`, key);
        update({ configured: true });
      }
      const items = await listLlmModels(provider, cfg.baseUrl);
      setModels(items);
      setNotice(`已获取 ${items.length} 个模型`);
    } catch (e) {
      setNotice((e as AppError)?.message || String(e));
    } finally {
      setListing(false);
    }
  };
  return (
    <div className="settings-card llm-pane">
      <SettingRow title="API Key" desc="保存到本地 SQLite">
        <input
          type="password"
          value={key}
          onChange={(e) => setKey(e.target.value)}
          placeholder={
            cfg.configured ? "已配置 · 输入新密钥可替换" : "请输入 API Key"
          }
        />
      </SettingRow>
      <SettingRow title="Base URL">
        <input
          value={cfg.baseUrl}
          onChange={(e) => update({ baseUrl: e.target.value })}
        />
      </SettingRow>
      <SettingRow title="模型 ID" desc="允许手动填写，避免模型版本写死">
        <input
          value={cfg.model}
          onChange={(e) => update({ model: e.target.value })}
        />
      </SettingRow>
      <div className="model-discovery">
        <button
          className="button small"
          disabled={listing || (!key && !cfg.configured) || !cfg.baseUrl.trim()}
          onClick={() => void fetchModels()}
        >
          {listing ? "正在获取…" : "获取服务器模型列表"}
        </button>
        {models.length > 0 && (
          <div className="model-discovery-list">
            {models.map((model, index) => {
              const id = String(model.id || model.name || `model-${index}`);
              const selected = id === cfg.model;
              return (
                <button
                  key={`${id}-${index}`}
                  className={clsx("model-discovery-item", selected && "active")}
                  onClick={() => update({ model: id })}
                >
                  <b>{id}</b>
                  <small>
                    {Object.entries(model)
                      .filter(([key]) => key !== "id" && key !== "name")
                      .slice(0, 3)
                      .map(([key, value]) => `${key}: ${String(value)}`)
                      .join(" · ") || "服务器未提供参数详情"}
                  </small>
                </button>
              );
            })}
          </div>
        )}
      </div>
      <SettingRow title="最大输出 token">
        <input
          type="number"
          min={64}
          max={16384}
          value={cfg.maxOutputTokens}
          onChange={(e) => update({ maxOutputTokens: +e.target.value })}
        />
      </SettingRow>
      <SettingRow
        title="模型思考"
        desc="关闭后跳过推理过程，整理更快；开启后更稳，但会变慢"
      >
        <Toggle
          on={cfg.thinking}
          onChange={(thinking) => update({ thinking })}
        />
      </SettingRow>
      <div className="provider-footer">
        <span className={notice.includes("成功") ? "success" : "error"}>
          {notice}
        </span>
        <button
          className="button primary"
          disabled={busy || (!key && !cfg.configured)}
          onClick={save}
        >
          {busy ? "正在测试…" : "保存并测试连接"}
        </button>
      </div>
    </div>
  );
}
export function LlmSettings() {
  const settings = useAppStore((s) => s.settings);
  const provider = settings.llmProvider;
  const patch = useAppStore((s) => s.patchSettings);
  return (
    <>
      <SettingsHeader
        title="大模型"
        desc="支持 OpenAI Chat Completions API 与 Claude Messages API"
      />
      <div className="settings-card" style={{ marginBottom: 16 }}>
        <SettingRow
          title="启用大模型格式化"
          desc="关闭后停用气泡中的手动与自动 AI 格式化"
        >
          <Toggle
            on={settings.llmEnabled}
            onChange={(llmEnabled) => patch({ llmEnabled })}
          />
        </SettingRow>
      </div>
      <div className="llm-switch">
        <button
          className={provider === "openai" ? "active" : ""}
          onClick={() => patch({ llmProvider: "openai" })}
        >
          <Sparkle />
          OpenAI Chat Completions<small>POST /v1/chat/completions</small>
        </button>
        <button
          className={provider === "claude" ? "active" : ""}
          onClick={() => patch({ llmProvider: "claude" })}
        >
          <Sparkle />
          Claude Messages<small>POST /v1/messages · 无状态</small>
        </button>
      </div>
      <LlmProviderPane provider={provider} />
    </>
  );
}
