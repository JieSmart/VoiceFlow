import { useEffect, useState } from "react";
import { HardDrive } from "@phosphor-icons/react";
import {
  chooseDataDirectory,
  clearRecords,
  getDataDirectory,
  migrateDataDirectory,
  quitForDataDirectoryRestart,
} from "../api";
import { useAppStore } from "../store";
import type {
  AppError,
  DataDirectoryChoice,
  DataDirectoryInfo,
  DataMigrationResult,
} from "../types";
import { contextMenuProps, copyMenuItems } from "../ui/contextMenu";
import { SettingsHeader, SettingRow } from "../ui/primitives";

export function PrivacySettings() {
  const [info, setInfo] = useState<DataDirectoryInfo | null>(null);
  const [choice, setChoice] = useState<DataDirectoryChoice | null>(null);
  const [result, setResult] = useState<DataMigrationResult | null>(null);
  const [busy, setBusy] = useState(false);
  const [notice, setNotice] = useState("");
  const [noticeKind, setNoticeKind] = useState<"error" | "success">("error");
  const settings = useAppStore((s) => s.settings);
  const patch = useAppStore((s) => s.patchSettings);
  const replaceRecords = useAppStore((s) => s.replaceRecords);

  useEffect(() => {
    let disposed = false;
    void getDataDirectory()
      .then((value) => !disposed && setInfo(value))
      .catch((error: AppError) => {
        if (!disposed) {
          setNoticeKind("error");
          setNotice(error.message || "无法读取数据目录");
        }
      });
    return () => { disposed = true; };
  }, []);

  const choose = async () => {
    setNotice("");
    setNoticeKind("error");
    setResult(null);
    try {
      const selected = await chooseDataDirectory();
      if (selected) setChoice(selected);
    } catch (error) {
      setNoticeKind("error");
      setNotice((error as AppError)?.message || "无法选择文件夹");
    }
  };

  const migrate = async () => {
    if (!choice) return;
    setBusy(true);
    setNotice("");
    setNoticeKind("error");
    try {
      const migrated = await migrateDataDirectory(choice.parentPath);
      setResult(migrated);
      setChoice(null);
    } catch (error) {
      setNoticeKind("error");
      setNotice((error as AppError)?.message || "数据目录迁移失败");
    } finally {
      setBusy(false);
    }
  };

  return (
    <>
      <SettingsHeader title="隐私与存储" desc="决定语流数据在本机的保存位置" />
      <div className="settings-card">
        <SettingRow title="记录保留时长" desc="超期的转写记录会在启动和保存设置时自动清理">
          <select
            value={String(settings.retentionDays)}
            onChange={(e) => patch({ retentionDays: Number(e.target.value) })}
          >
            <option value="7">7 天</option>
            <option value="30">30 天</option>
            <option value="90">90 天</option>
            <option value="0">永久</option>
          </select>
        </SettingRow>
        <SettingRow title="清空全部记录" desc="删除本机转写记录，不可恢复">
          <button
            className="button small"
            onClick={() =>
              void clearRecords().then(() => {
                replaceRecords([]);
                setNoticeKind("success");
                setNotice("已清空输入记录");
              })
            }
          >
            立即清空
          </button>
        </SettingRow>
      </div>
      <div className="settings-card storage-settings spaced">
        <SettingRow
          title="应用数据目录"
          desc="数据库、API Key、本地模型、推理运行时与临时缓存统一保存在这里"
        >
          <button className="secondary-button" onClick={choose} disabled={busy || !!result}>
            <HardDrive size={17} />
            选择新位置
          </button>
        </SettingRow>
        <div className="storage-path-block">
          <small>{info?.isCustom ? "当前使用自定义目录" : "当前使用系统默认目录"}</small>
          <code
            {...contextMenuProps(
              "data-current-path",
              copyMenuItems([{ id: "copy-path", label: "复制路径", text: info?.currentPath || "" }]),
            )}
          >
            {info?.currentPath || "正在读取…"}
          </code>
          {info?.isCustom && <small>系统默认目录：{info.defaultPath}</small>}
        </div>
      </div>

      {choice && (
        <div className="settings-card spaced storage-confirm">
          <b>确认迁移到新目录</b>
          <p>语流将在所选位置创建“语流”子目录，并复制当前全部应用数据。</p>
          <code
            {...contextMenuProps(
              "data-target-path",
              copyMenuItems([{ id: "copy-path", label: "复制路径", text: choice.targetPath }]),
            )}
          >
            {choice.targetPath}
          </code>
          <small>目标子目录必须为空或不存在。迁移成功后旧目录会保留，重启应用后生效。</small>
          <div className="storage-actions">
            <button className="secondary-button" onClick={() => setChoice(null)} disabled={busy}>取消</button>
            <button className="primary-button" onClick={migrate} disabled={busy}>
              {busy ? "正在迁移…" : "开始迁移"}
            </button>
          </div>
        </div>
      )}

      {busy && (
        <div className="restart-required-backdrop">
          <div className="settings-card storage-result">
            <b>正在迁移应用数据…</b>
            <small>请保持语流运行，不要断开目标磁盘。</small>
          </div>
        </div>
      )}

      {result && (
        <div className="restart-required-backdrop">
          <div className="settings-card storage-result success">
            <b>迁移完成，请重启语流</b>
            <small>重启后使用：{result.newPath}</small>
            <small>旧目录仍保留：{result.oldPath}</small>
            <p>为避免新旧目录产生不同数据，重启前将暂停录音和设置操作。</p>
            <button className="primary-button" onClick={() => void quitForDataDirectoryRestart()}>
              退出语流，随后手动重新打开
            </button>
          </div>
        </div>
      )}
      {notice && (
        <div className={`settings-card spaced storage-result ${noticeKind}`}>{notice}</div>
      )}
    </>
  );
}
