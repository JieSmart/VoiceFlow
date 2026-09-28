import { SettingsHeader, SettingRow, Toggle } from "../ui/primitives";
import { useAppStore } from "../store";

export function HotwordBehaviorSettings() {
  const settings = useAppStore((s) => s.settings);
  const patch = useAppStore((s) => s.patchSettings);
  const setRoute = useAppStore((s) => s.setRoute);
  const close = useAppStore((s) => s.closeSettings);
  return (
    <>
      <SettingsHeader title="热词行为" desc="控制匹配、替换和新建热词的默认优先级" />
      <div className="settings-card">
        <SettingRow title="启用热词引擎" desc="关闭后不再改写识别结果，也不把热词发给 ASR">
          <Toggle
            on={settings.hotwordsEnabled}
            onChange={(hotwordsEnabled) => patch({ hotwordsEnabled })}
          />
        </SettingRow>
        <SettingRow title="区分大小写" desc="关闭后「OKR」与「okr」视为同一热词">
          <Toggle
            on={settings.hotwordCaseSensitive}
            onChange={(hotwordCaseSensitive) => patch({ hotwordCaseSensitive })}
          />
        </SettingRow>
        <SettingRow title="新建热词默认优先级">
          <div className="inline-secret">
            <input
              type="range"
              min={1}
              max={5}
              value={settings.defaultHotwordPriority}
              onChange={(e) => patch({ defaultHotwordPriority: Number(e.target.value) })}
            />
            <span>{settings.defaultHotwordPriority}</span>
          </div>
        </SettingRow>
        <SettingRow title="管理热词列表">
          <button
            className="button small primary"
            onClick={() => {
              close();
              setRoute("hotwords");
            }}
          >
            前往热词库
          </button>
        </SettingRow>
      </div>
    </>
  );
}
