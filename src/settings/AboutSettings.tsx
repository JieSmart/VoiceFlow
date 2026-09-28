import { SettingsHeader, SettingRow } from "../ui/primitives";

export function AboutSettings() {
  return (
    <>
      <SettingsHeader title="关于语流" desc="Windows 桌面语音输入工具" />
      <div className="settings-card">
        <SettingRow title="版本" desc="说了就上屏">
          <span className="chip">v0.2.1</span>
        </SettingRow>
        <SettingRow title="技术栈" desc="本机运行，无账号与云同步">
          <div className="chip-wrap">
            {["Tauri", "React", "Sherpa", "Whisper", "SQLite"].map((item) => (
              <span className="chip" key={item}>
                {item}
              </span>
            ))}
          </div>
        </SettingRow>
      </div>
    </>
  );
}
