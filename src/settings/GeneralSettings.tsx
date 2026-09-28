import { SettingsHeader, SettingRow, Toggle } from "../ui/primitives";
import { useAppStore } from "../store";

export function GeneralSettings() {
  const s = useAppStore((x) => x.settings),
    patch = useAppStore((x) => x.patchSettings);
  return (
    <>
      <SettingsHeader title="通用" desc="启动、托盘与上屏行为" />
      <div className="settings-card">
        <SettingRow title="开机自动启动" desc="登录 Windows 后在后台待命">
          <Toggle
            on={s.autoStart}
            onChange={(autoStart) => patch({ autoStart })}
          />
        </SettingRow>
        <SettingRow
          title="开机启动后隐藏主窗口"
          desc="仅登录自启动时进托盘；从开始菜单或桌面打开仍显示主窗口"
        >
          <Toggle
            on={s.hideOnStart}
            onChange={(hideOnStart) => patch({ hideOnStart })}
          />
        </SettingRow>
        <SettingRow
          title="识别完成后直接上屏"
          desc="开启后跳过结果确认，识别完成即写入原输入框"
        >
          <Toggle
            on={s.directInsert}
            onChange={(directInsert) => patch({ directInsert })}
          />
        </SettingRow>
        <SettingRow
          title="自动 AI 格式化"
          desc="开启后先整理识别结果再上屏；未配置或处理失败时使用原文"
        >
          <Toggle
            on={s.autoFormat}
            onChange={(autoFormat) => patch({ autoFormat })}
          />
        </SettingRow>
        <SettingRow
          title="静音后自动结束并进入结果"
          desc="说话后持续静音约 1.5 秒则自动结束；关闭后仅能手动结束"
        >
          <Toggle
            on={s.autoEndOnSilence}
            onChange={(autoEndOnSilence) => patch({ autoEndOnSilence })}
          />
        </SettingRow>
        <SettingRow title="语音气泡样式">
          <div className="segment">
            {(["A", "B", "C"] as const).map((x) => (
              <button
                key={x}
                className={s.bubbleStyle === x ? "active" : ""}
                onClick={() => patch({ bubbleStyle: x })}
              >
                {x}
              </button>
            ))}
          </div>
        </SettingRow>
        <SettingRow
          title="播放录音提示音"
          desc="开始和结束录音时各响一声短促反馈音"
        >
          <Toggle on={s.sounds} onChange={(sounds) => patch({ sounds })} />
        </SettingRow>
      </div>
    </>
  );
}
