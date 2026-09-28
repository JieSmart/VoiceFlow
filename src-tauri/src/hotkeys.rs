use serde_json::Value;
use std::time::{Duration, Instant};

pub const DEFAULT_WAKE: &str = "Ctrl+Shift+Space";
pub const DEFAULT_PANEL: &str = "Ctrl+Shift+V";
pub const DEFAULT_AI: &str = "Ctrl+Alt+G";
pub const DEFAULT_MUTE: &str = "Ctrl+Shift+M";
pub const DEFAULT_HOLD_MS: u64 = 220;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HotkeyAction {
    Wake,
    Panel,
    Ai,
    Mute,
    Escape,
    ReviewEnter,
    ReviewAiEnter,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WakeMode {
    Toggle,
    Hold,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HotkeyConfig {
    pub wake: String,
    pub panel: String,
    pub ai: String,
    pub mute: String,
    pub mode: WakeMode,
    pub hold_ms: u64,
}

impl Default for HotkeyConfig {
    fn default() -> Self {
        Self {
            wake: DEFAULT_WAKE.into(),
            panel: DEFAULT_PANEL.into(),
            ai: DEFAULT_AI.into(),
            mute: DEFAULT_MUTE.into(),
            mode: WakeMode::Toggle,
            hold_ms: DEFAULT_HOLD_MS,
        }
    }
}

pub fn normalize_accelerator(value: &str) -> String {
    let mut parts: Vec<&str> = value
        .split('+')
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .collect();
    if parts.is_empty() {
        return String::new();
    }
    let key = normalize_key(parts.pop().unwrap_or_default());
    let mut mods = Vec::new();
    for part in parts {
        if let Some(modifier) = normalize_mod(part) {
            if !mods.contains(&modifier) {
                mods.push(modifier);
            }
        }
    }
    mods.sort_by_key(|item| match *item {
        "Ctrl" => 0,
        "Shift" => 1,
        "Alt" => 2,
        "Meta" => 3,
        _ => 4,
    });
    let mut parts = mods
        .into_iter()
        .map(str::to_owned)
        .collect::<Vec<_>>();
    parts.push(key);
    parts.join("+")
}

fn normalize_mod(value: &str) -> Option<&'static str> {
    match value.to_ascii_lowercase().as_str() {
        "ctrl" | "control" | "ctl" => Some("Ctrl"),
        "shift" => Some("Shift"),
        "alt" | "option" => Some("Alt"),
        "meta" | "cmd" | "command" | "win" | "super" => Some("Meta"),
        _ => None,
    }
}

fn normalize_key(value: &str) -> String {
    if value.eq_ignore_ascii_case("space") || value == " " {
        return "Space".into();
    }
    if value.len() == 1 {
        return value.to_ascii_uppercase();
    }
    let mut chars = value.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

pub fn accelerators_equal(left: &str, right: &str) -> bool {
    normalize_accelerator(left).eq_ignore_ascii_case(&normalize_accelerator(right))
}

pub fn hotkey_config(settings: &Value) -> HotkeyConfig {
    let hotkeys = settings.get("hotkeys").cloned().unwrap_or(Value::Null);
    let hold_ms = hotkeys
        .get("holdMs")
        .and_then(Value::as_u64)
        .unwrap_or(DEFAULT_HOLD_MS)
        .clamp(100, 500);
    HotkeyConfig {
        wake: normalize_accelerator(
            hotkeys
                .get("wake")
                .and_then(Value::as_str)
                .unwrap_or(DEFAULT_WAKE),
        ),
        panel: normalize_accelerator(
            hotkeys
                .get("panel")
                .and_then(Value::as_str)
                .unwrap_or(DEFAULT_PANEL),
        ),
        ai: normalize_accelerator(
            hotkeys
                .get("ai")
                .and_then(Value::as_str)
                .unwrap_or(DEFAULT_AI),
        ),
        mute: normalize_accelerator(
            hotkeys
                .get("mute")
                .and_then(Value::as_str)
                .unwrap_or(DEFAULT_MUTE),
        ),
        mode: if hotkeys
            .get("mode")
            .and_then(Value::as_str)
            .is_some_and(|value| value.eq_ignore_ascii_case("hold"))
        {
            WakeMode::Hold
        } else {
            WakeMode::Toggle
        },
        hold_ms,
    }
}

pub fn classify_shortcut(shortcut: &str, config: &HotkeyConfig) -> Option<HotkeyAction> {
    let value = normalize_accelerator(shortcut);
    if value.eq_ignore_ascii_case("Escape") {
        return Some(HotkeyAction::Escape);
    }
    if accelerators_equal(&value, "Enter") {
        return Some(HotkeyAction::ReviewEnter);
    }
    if accelerators_equal(&value, "Ctrl+Enter") {
        return Some(HotkeyAction::ReviewAiEnter);
    }
    if accelerators_equal(&value, &config.wake) {
        return Some(HotkeyAction::Wake);
    }
    if accelerators_equal(&value, &config.panel) {
        return Some(HotkeyAction::Panel);
    }
    if accelerators_equal(&value, &config.ai) {
        return Some(HotkeyAction::Ai);
    }
    if accelerators_equal(&value, &config.mute) {
        return Some(HotkeyAction::Mute);
    }
    None
}

pub fn registered_user_shortcuts(config: &HotkeyConfig) -> Vec<String> {
    let mut items = vec![
        config.wake.clone(),
        config.panel.clone(),
        config.ai.clone(),
        config.mute.clone(),
    ];
    items.sort();
    items.dedup();
    items
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HoldReleaseAction {
    Finish,
    Cancel,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HoldReleasePending {
    pub session_id: Option<String>,
    pub action: HoldReleaseAction,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WakeSessionBehavior {
    StopOrFinish,
    ToggleCancelHoldIgnore,
    IgnoreWake,
}

pub fn hold_release_action(pressed_at: Instant, hold_ms: u64, now: Instant) -> HoldReleaseAction {
    if now.duration_since(pressed_at) < Duration::from_millis(hold_ms) {
        HoldReleaseAction::Cancel
    } else {
        HoldReleaseAction::Finish
    }
}

pub fn session_wake_behavior(state: &str) -> WakeSessionBehavior {
    match state {
        "reviewing" => WakeSessionBehavior::ToggleCancelHoldIgnore,
        "inserting" | "inserted" | "finalizing" => WakeSessionBehavior::IgnoreWake,
        _ => WakeSessionBehavior::StopOrFinish,
    }
}

pub fn arm_hold_release_pending(
    pending: &mut Option<HoldReleasePending>,
    action: HoldReleaseAction,
) {
    *pending = Some(HoldReleasePending {
        session_id: None,
        action,
    });
}

pub fn bind_hold_release_pending(pending: &mut Option<HoldReleasePending>, session_id: &str) {
    match pending {
        Some(item) if item.session_id.is_none() => {
            item.session_id = Some(session_id.to_owned());
        }
        Some(item) if item.session_id.as_deref() != Some(session_id) => {
            *pending = None;
        }
        _ => {}
    }
}

pub fn take_hold_release_pending(
    pending: &mut Option<HoldReleasePending>,
    session_id: &str,
) -> Option<HoldReleaseAction> {
    match pending.as_ref() {
        Some(item)
            if item.session_id.is_none() || item.session_id.as_deref() == Some(session_id) =>
        {
            pending.take().map(|item| item.action)
        }
        _ => None,
    }
}

pub fn clear_hold_release_pending(pending: &mut Option<HoldReleasePending>, session_id: Option<&str>) {
    let matches = match (pending.as_ref(), session_id) {
        (None, _) => false,
        (Some(_), None) => true,
        (Some(item), Some(id)) => item.session_id.is_none() || item.session_id.as_deref() == Some(id),
    };
    if matches {
        *pending = None;
    }
}

pub fn ensure_hotkey_defaults(settings: &mut Value) -> bool {
    if !settings.is_object() {
        *settings = serde_json::json!({});
    }
    let defaults = HotkeyConfig::default();
    let Some(object) = settings.as_object_mut() else {
        return false;
    };
    let mut changed = false;
    if object.get("hotwordsEnabled").is_none() {
        object.insert("hotwordsEnabled".into(), Value::Bool(true));
        changed = true;
    }
    if object.get("hotwordCaseSensitive").is_none() {
        object.insert("hotwordCaseSensitive".into(), Value::Bool(false));
        changed = true;
    }
    if object.get("defaultHotwordPriority").is_none() {
        object.insert("defaultHotwordPriority".into(), Value::from(3));
        changed = true;
    }
    let hotkeys = object
        .entry("hotkeys".to_owned())
        .or_insert_with(|| serde_json::json!({}));
    if let Some(map) = hotkeys.as_object_mut() {
        for (key, value) in [
            ("wake", defaults.wake),
            ("panel", defaults.panel),
            ("ai", defaults.ai),
            ("mute", defaults.mute),
        ] {
            if map.get(key).and_then(Value::as_str).unwrap_or("").is_empty() {
                map.insert(key.into(), Value::String(value));
                changed = true;
            }
        }
        if map.get("mode").and_then(Value::as_str).unwrap_or("").is_empty() {
            map.insert("mode".into(), Value::String("toggle".into()));
            changed = true;
        }
        if map.get("holdMs").and_then(Value::as_u64).is_none() {
            map.insert("holdMs".into(), Value::from(DEFAULT_HOLD_MS));
            changed = true;
        }
    }
    changed
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_user_shortcuts_without_treating_panel_as_wake() {
        let config = HotkeyConfig::default();
        assert_eq!(
            classify_shortcut("Ctrl+Shift+Space", &config),
            Some(HotkeyAction::Wake)
        );
        assert_eq!(
            classify_shortcut("ctrl+shift+v", &config),
            Some(HotkeyAction::Panel)
        );
        assert_eq!(
            classify_shortcut("Ctrl+Alt+G", &config),
            Some(HotkeyAction::Ai)
        );
        assert_eq!(classify_shortcut("Enter", &config), Some(HotkeyAction::ReviewEnter));
        assert_eq!(
            classify_shortcut("Ctrl+Enter", &config),
            Some(HotkeyAction::ReviewAiEnter)
        );
        assert_eq!(classify_shortcut("Ctrl+Shift+Y", &config), None);
        assert_eq!(classify_shortcut("F24", &config), None);
    }

    #[test]
    fn hold_release_pending_binds_to_the_listening_session() {
        let mut pending = None;
        arm_hold_release_pending(&mut pending, HoldReleaseAction::Cancel);
        assert_eq!(
            take_hold_release_pending(&mut pending, "session-a"),
            Some(HoldReleaseAction::Cancel)
        );
        assert!(pending.is_none());

        arm_hold_release_pending(&mut pending, HoldReleaseAction::Finish);
        bind_hold_release_pending(&mut pending, "session-a");
        assert_eq!(take_hold_release_pending(&mut pending, "session-b"), None);
        assert_eq!(
            take_hold_release_pending(&mut pending, "session-a"),
            Some(HoldReleaseAction::Finish)
        );
    }

    #[test]
    fn hold_release_pending_is_cleared_on_failure_or_new_press() {
        let mut pending = None;
        arm_hold_release_pending(&mut pending, HoldReleaseAction::Cancel);
        clear_hold_release_pending(&mut pending, None);
        assert!(pending.is_none());

        arm_hold_release_pending(&mut pending, HoldReleaseAction::Finish);
        bind_hold_release_pending(&mut pending, "session-a");
        clear_hold_release_pending(&mut pending, Some("session-b"));
        assert!(pending.is_some());
        clear_hold_release_pending(&mut pending, Some("session-a"));
        assert!(pending.is_none());
    }

    #[test]
    fn reviewing_cancels_toggle_while_inserting_ignores_wake() {
        assert_eq!(
            session_wake_behavior("reviewing"),
            WakeSessionBehavior::ToggleCancelHoldIgnore
        );
        assert_eq!(
            session_wake_behavior("inserting"),
            WakeSessionBehavior::IgnoreWake
        );
        assert_eq!(
            session_wake_behavior("inserted"),
            WakeSessionBehavior::IgnoreWake
        );
        assert_eq!(
            session_wake_behavior("capturing"),
            WakeSessionBehavior::StopOrFinish
        );
    }

    #[test]
    fn hold_release_cancels_short_presses() {
        let start = Instant::now();
        assert_eq!(
            hold_release_action(start, 220, start + Duration::from_millis(80)),
            HoldReleaseAction::Cancel
        );
        assert_eq!(
            hold_release_action(start, 220, start + Duration::from_millis(300)),
            HoldReleaseAction::Finish
        );
    }

    #[test]
    fn fills_missing_hotkey_defaults() {
        let mut settings = serde_json::json!({"directInsert": true});
        assert!(ensure_hotkey_defaults(&mut settings));
        assert_eq!(settings["hotkeys"]["wake"], DEFAULT_WAKE);
        assert_eq!(settings["hotkeys"]["ai"], DEFAULT_AI);
        assert_eq!(settings["hotwordsEnabled"], true);
        assert!(!ensure_hotkey_defaults(&mut settings));
    }
}
