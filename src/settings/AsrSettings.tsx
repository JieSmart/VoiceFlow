import { useEffect, useState } from "react";
import { Check, Cloud, Cpu, HardDrive, Lightning, LockKey } from "@phosphor-icons/react";
import clsx from "clsx";
import {
  audioDevices,
  getComputeStatus,
  listLocalModels,
  secretConfigured,
  setSecret,
  testAsr,
} from "../api";
import { resolvedMicrophoneId } from "../audioDevices";
import { useAppStore } from "../store";
import type { AppError } from "../types";
import { SettingsHeader, SettingRow, Toggle } from "../ui/primitives";
import { LocalModelManager } from "./LocalModelManager";

export function AsrSettings() {
  const s = useAppStore((x) => x.settings),
    patch = useAppStore((x) => x.patchSettings);
  const [devices, setDevices] = useState<
      { id: string; name: string; isDefault: boolean }[]
    >([]),
    [key, setKey] = useState(""),
    [xunfei, setXunfei] = useState({ appId: "", apiKey: "", apiSecret: "" }),
    [tencent, setTencent] = useState({ appId: "", secretId: "", secretKey: "" }),
    [tencentConfigured, setTencentConfigured] = useState({ appId: false, secretId: false, secretKey: false }),
    [computeStatus, setComputeStatus] = useState("正在检测计算设备…"),
    [saved, setSaved] = useState(""),
    [testing, setTesting] = useState(false);
  useEffect(() => {
    void audioDevices()
      .then((next) => {
        setDevices(next);
        const saved = useAppStore.getState().settings.microphoneId;
        const resolved = resolvedMicrophoneId(saved, next);
        if (resolved && resolved !== saved) {
          useAppStore.getState().patchSettings({ microphoneId: resolved });
        }
      })
      .catch(() => {});
  }, []);
  useEffect(() => {
    void getComputeStatus().then((status) => setComputeStatus(status.message)).catch(() => setComputeStatus("计算设备检测失败"));
  }, [s.computeDevice]);
  useEffect(() => {
    if (s.asrProvider !== "tencent") return;
    void Promise.all([
      secretConfigured("asr-tencent-app-id"),
      secretConfigured("asr-tencent-secret-id"),
      secretConfigured("asr-tencent-secret-key"),
    ]).then(([appId, secretId, secretKey]) => {
      setTencentConfigured({ appId, secretId, secretKey });
    }).catch(() => {
      setTencentConfigured({ appId: false, secretId: false, secretKey: false });
    });
  }, [s.asrProvider]);
  return (
    <>
      <SettingsHeader
        title="语音识别"
        desc="选择云端或本地 ASR 引擎；云端会上传原始音频"
      />
      <div className="provider-grid">
        {[
          {
            id: "openai",
            name: "OpenAI Realtime",
            desc: "持续音频流，增量文字返回",
            icon: Cloud,
          },
          {
            id: "sherpa-streaming",
            name: "Sherpa 本地流式",
            desc: "Zipformer 在线解码，边说边出字",
            icon: Cpu,
          },
          {
            id: "xunfei",
            name: "讯飞听见",
            desc: "中文流式识别与热词",
            icon: Lightning,
          },
          {
            id: "tencent",
            name: "腾讯云 ASR",
            desc: "实时语音识别与增量结果",
            icon: Cloud,
          },
          {
            id: "qwen3-asr",
            name: "Qwen3-ASR",
            desc: "1.7B 本地离线，多语言与方言",
            icon: Cpu,
          },
          {
            id: "faster-whisper",
            name: "Whisper 本地",
            desc: "内置运行时，本地多语言",
            icon: HardDrive,
          },
        ].map((p) => (
          <button
            className={clsx("provider", s.asrProvider === p.id && "active")}
            onClick={async () => {
              const provider = p.id as typeof s.asrProvider;
              if (
                provider === "qwen3-asr" ||
                provider === "faster-whisper" ||
                provider === "sherpa-streaming"
              ) {
                const installed = (await listLocalModels()).filter(
                  (model) => model.engine === provider && model.status === "done",
                );
                const selected = installed.find(
                  (model) =>
                    model.id === s.localModelId && s.localModelEngine === provider,
                ) ?? installed[0];
                patch({
                  asrProvider: provider,
                  localModelId: selected?.id,
                  localModelPath: selected?.path,
                  localModelEngine: selected
                    ? provider
                    : undefined,
                });
                return;
              }
              patch({ asrProvider: provider });
            }}
            key={p.id}
          >
            <p.icon />
            <b>{p.name}</b>
            <small>{p.desc}</small>
            {s.asrProvider === p.id && <Check className="provider-check" />}
          </button>
        ))}
      </div>
      {(s.asrProvider === "qwen3-asr" ||
        s.asrProvider === "faster-whisper" ||
        s.asrProvider === "sherpa-streaming") && (
        <LocalModelManager engine={s.asrProvider} />
      )}
      <div className="settings-card spaced">
        {s.asrProvider === "openai" && (
          <SettingRow title="OpenAI API Key" desc="可与 LLM 密钥分开配置">
            <div className="inline-secret">
              <input
                type="password"
                value={key}
                onChange={(e) => setKey(e.target.value)}
                placeholder="sk-…"
              />
              <button
                className="button small"
                disabled={!key}
                onClick={async () => {
                  await setSecret("asr-openai", key);
                  setKey("");
                  setSaved("已保存");
                }}
              >
                保存
              </button>
              {saved && <span>{saved}</span>}
            </div>
          </SettingRow>
        )}
        {s.asrProvider === "xunfei" && (
          <>
            <SettingRow title="讯飞 AppID"><input value={xunfei.appId} onChange={(e) => setXunfei({ ...xunfei, appId: e.target.value })} /></SettingRow>
            <SettingRow title="讯飞 API Key"><input type="password" value={xunfei.apiKey} onChange={(e) => setXunfei({ ...xunfei, apiKey: e.target.value })} /></SettingRow>
            <SettingRow title="讯飞 API Secret"><input type="password" value={xunfei.apiSecret} onChange={(e) => setXunfei({ ...xunfei, apiSecret: e.target.value })} /></SettingRow>
            <SettingRow title="凭据">
              <button className="button small" disabled={!xunfei.appId || !xunfei.apiKey || !xunfei.apiSecret} onClick={async () => {
                await Promise.all([
                  setSecret("asr-xunfei-app-id", xunfei.appId),
                  setSecret("asr-xunfei-api-key", xunfei.apiKey),
                  setSecret("asr-xunfei-api-secret", xunfei.apiSecret),
                ]);
                setXunfei({ appId: "", apiKey: "", apiSecret: "" });
                setSaved("已保存");
              }}>保存讯飞凭据</button>
            </SettingRow>
          </>
        )}
        {s.asrProvider === "tencent" && (
          <>
            <SettingRow title="腾讯云 AppID"><input value={tencent.appId} placeholder={tencentConfigured.appId ? "已配置 · 输入新值可替换" : "请输入腾讯云 AppID"} onChange={(e) => setTencent({ ...tencent, appId: e.target.value })} /></SettingRow>
            <SettingRow title="腾讯云 SecretId"><input value={tencent.secretId} placeholder={tencentConfigured.secretId ? "已配置 · 输入新值可替换" : "请输入腾讯云 SecretId"} onChange={(e) => setTencent({ ...tencent, secretId: e.target.value })} /></SettingRow>
            <SettingRow title="腾讯云 SecretKey"><input type="password" value={tencent.secretKey} placeholder={tencentConfigured.secretKey ? "已配置 · 输入新值可替换" : "请输入腾讯云 SecretKey"} onChange={(e) => setTencent({ ...tencent, secretKey: e.target.value })} /></SettingRow>
            <SettingRow title="凭据" desc={Object.values(tencentConfigured).every(Boolean) ? "已完整配置并保存到本地" : "尚未完整配置"}>
              <button className="button small" disabled={!tencent.appId || !tencent.secretId || !tencent.secretKey} onClick={async () => {
                await Promise.all([
                  setSecret("asr-tencent-app-id", tencent.appId),
                  setSecret("asr-tencent-secret-id", tencent.secretId),
                  setSecret("asr-tencent-secret-key", tencent.secretKey),
                ]);
                setTencent({ appId: "", secretId: "", secretKey: "" });
                setTencentConfigured({ appId: true, secretId: true, secretKey: true });
                setSaved("已保存");
              }}>保存腾讯云凭据</button>
            </SettingRow>
          </>
        )}
        {(s.asrProvider === "qwen3-asr" || s.asrProvider === "faster-whisper" || s.asrProvider === "sherpa-streaming") && (
          <SettingRow title="本地推理设备" desc={computeStatus}>
            <select value={s.computeDevice} onChange={(e) => patch({ computeDevice: e.target.value as typeof s.computeDevice })}>
              <option value="auto">自动（优先 GPU）</option>
              <option value="cuda">NVIDIA CUDA</option>
              <option value="cpu">CPU</option>
            </select>
          </SettingRow>
        )}
        {s.asrProvider === "openai" && (
          <>
            <SettingRow title="Base URL">
              <input
                value={s.asrBaseUrl}
                onChange={(e) => patch({ asrBaseUrl: e.target.value })}
              />
            </SettingRow>
            <SettingRow title="模型 ID" desc="Realtime 转写模型，例如 gpt-4o-mini-transcribe">
              <input
                value={s.asrModel}
                onChange={(e) => patch({ asrModel: e.target.value })}
              />
            </SettingRow>
          </>
        )}
        <SettingRow title="录音设备" desc="下次唤醒时生效；正在录音时不会热切换。">
          <select
            value={s.microphoneId || ""}
            onChange={(e) => patch({ microphoneId: e.target.value })}
          >
            {devices.map((d) => (
              <option value={d.id} key={d.id}>
                {d.name}
                {d.isDefault ? "（默认）" : ""}
              </option>
            ))}
          </select>
        </SettingRow>
        <SettingRow title="采样格式" desc="内部统一转换，适配所有引擎">
          <span className="chip">16 kHz · 单声道 PCM</span>
        </SettingRow>
        <div className="provider-footer">
          <span className={saved.includes("成功") ? "success" : "error"}>
            {saved}
          </span>
          <button
            className="button"
            disabled={testing}
            onClick={async () => {
              setTesting(true);
              setSaved("");
              try {
                await testAsr({
                  provider: s.asrProvider,
                  model:
                    s.asrProvider === "qwen3-asr" ||
                    s.asrProvider === "faster-whisper" ||
                    s.asrProvider === "sherpa-streaming"
                      ? s.localModelEngine === s.asrProvider
                        ? s.localModelPath
                        : undefined
                      : s.asrModel,
                  baseUrl:
                    s.asrProvider === "qwen3-asr" ||
                    s.asrProvider === "faster-whisper" ||
                    s.asrProvider === "sherpa-streaming"
                      ? undefined
                      : s.asrBaseUrl,
                  language: "zh",
                  computeDevice: s.computeDevice,
                });
                setSaved("连接成功");
              } catch (e) {
                setSaved((e as AppError)?.message || String(e));
              } finally {
                setTesting(false);
              }
            }}
          >
            {testing ? "正在测试…" : "测试连接"}
          </button>
        </div>
      </div>
    </>
  );
}
