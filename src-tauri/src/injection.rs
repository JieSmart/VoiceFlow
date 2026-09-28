use crate::{
    error::AppError,
    models::{FocusTarget, InjectionState, TranscriptSegment},
};
use enigo::{Direction, Enigo, Key, Keyboard, Settings};
use parking_lot::Mutex;
use std::{
    collections::HashMap,
    sync::{Arc, OnceLock},
};

#[derive(Default)]
struct ClipboardRestoreState {
    generation: u64,
    captured: bool,
    original: Option<String>,
}

static CLIPBOARD_RESTORE: OnceLock<Mutex<ClipboardRestoreState>> = OnceLock::new();

#[derive(Debug, Clone)]
pub struct InjectionTransaction {
    pub target: FocusTarget,
    pub committed_text: String,
    pub provisional_text: String,
}

#[derive(Clone, Default)]
pub struct InjectionManager(pub Arc<Mutex<HashMap<String, InjectionTransaction>>>);

impl InjectionManager {
    pub fn begin(&self, session_id: &str, target: FocusTarget) {
        self.0.lock().insert(
            session_id.to_owned(),
            InjectionTransaction {
                target,
                committed_text: String::new(),
                provisional_text: String::new(),
            },
        );
    }

    pub fn commit(
        &self,
        session_id: &str,
        segment: &TranscriptSegment,
    ) -> Result<InjectionState, AppError> {
        let mut sessions = self.0.lock();
        let tx = sessions
            .get_mut(session_id)
            .ok_or_else(|| AppError::new("session.missing", "录音会话不存在"))?;
        if !is_same_target(&tx.target) {
            return Ok(InjectionState {
                session_id: session_id.to_owned(),
                state: "focus_lost".into(),
                committed_text: tx.committed_text.clone(),
                message: Some("焦点已离开原输入框，实时写入已暂停".into()),
            });
        }
        let common = common_prefix_len(&tx.provisional_text, &segment.text);
        let previous_tail = tx.provisional_text.chars().count().saturating_sub(common);
        if previous_tail > 0 {
            send_backspaces(previous_tail)?;
        }
        let next_tail: String = segment.text.chars().skip(common).collect();
        if !next_tail.is_empty() {
            send_text(&next_tail)?;
        }
        if segment.stable {
            tx.committed_text.push_str(&segment.text);
            tx.provisional_text.clear();
        } else {
            tx.provisional_text = segment.text.clone();
        }
        Ok(InjectionState {
            session_id: session_id.to_owned(),
            state: if segment.stable {
                "replaced"
            } else {
                "streaming"
            }
            .into(),
            committed_text: format!("{}{}", tx.committed_text, tx.provisional_text),
            message: None,
        })
    }

    pub fn replace_all(
        &self,
        session_id: &str,
        segment: &TranscriptSegment,
    ) -> Result<InjectionState, AppError> {
        let mut sessions = self.0.lock();
        let tx = sessions
            .get_mut(session_id)
            .ok_or_else(|| AppError::new("session.missing", "录音会话不存在"))?;
        if !is_same_target(&tx.target) {
            return Ok(InjectionState {
                session_id: session_id.to_owned(),
                state: "focus_lost".into(),
                committed_text: format!("{}{}", tx.committed_text, tx.provisional_text),
                message: Some("焦点已离开原输入框，最终文字未能替换".into()),
            });
        }
        let previous_len = tx.committed_text.chars().count() + tx.provisional_text.chars().count();
        if previous_len > 0 {
            send_backspaces(previous_len)?;
        }
        if !segment.text.is_empty() {
            send_text(&segment.text)?;
        }
        tx.committed_text = segment.text.clone();
        tx.provisional_text.clear();
        Ok(InjectionState {
            session_id: session_id.to_owned(),
            state: "replaced".into(),
            committed_text: tx.committed_text.clone(),
            message: None,
        })
    }

    pub fn commit_review(
        &self,
        session_id: &str,
        segment: &TranscriptSegment,
    ) -> Result<InjectionState, AppError> {
        let current = self
            .0
            .lock()
            .get(session_id)
            .map(|tx| format!("{}{}", tx.committed_text, tx.provisional_text))
            .ok_or_else(|| AppError::new("session.missing", "录音会话不存在"))?;
        if !review_text_needs_replacement(&current, &segment.text) {
            return Ok(InjectionState {
                session_id: session_id.to_owned(),
                state: "inserted".into(),
                committed_text: current,
                message: None,
            });
        }
        self.replace_all(session_id, segment)
    }

    pub fn revert_all(&self, session_id: &str) -> Result<Option<InjectionState>, AppError> {
        let mut sessions = self.0.lock();
        let Some(tx) = sessions.get_mut(session_id) else {
            return Ok(None);
        };
        let current = format!("{}{}", tx.committed_text, tx.provisional_text);
        if current.is_empty() {
            return Ok(Some(InjectionState {
                session_id: session_id.to_owned(),
                state: "cancelled".into(),
                committed_text: String::new(),
                message: None,
            }));
        }
        if !is_same_target(&tx.target) {
            return Ok(Some(InjectionState {
                session_id: session_id.to_owned(),
                state: "focus_lost".into(),
                committed_text: current,
                message: Some("焦点已离开原输入框，无法撤回已实时上屏的文字".into()),
            }));
        }
        send_backspaces(current.chars().count())?;
        tx.committed_text.clear();
        tx.provisional_text.clear();
        Ok(Some(InjectionState {
            session_id: session_id.to_owned(),
            state: "cancelled".into(),
            committed_text: String::new(),
            message: None,
        }))
    }

    pub fn finish(&self, session_id: &str) -> Option<InjectionTransaction> {
        self.0.lock().remove(session_id)
    }
}

fn send_text(text: &str) -> Result<(), AppError> {
    let mut enigo = Enigo::new(&Settings::default())
        .map_err(|e| AppError::new("injection.init", e.to_string()))?;
    let mut clipboard = arboard::Clipboard::new()
        .map_err(|e| AppError::new("injection.clipboard", e.to_string()).fallback("copy"))?;
    let previous = clipboard.get_text().ok();
    clipboard
        .set_text(text.to_owned())
        .map_err(|e| AppError::new("injection.clipboard", e.to_string()).fallback("copy"))?;
    release_shift(&mut enigo)?;
    let paste = (|| {
        enigo.key(Key::Control, Direction::Press)?;
        let click = enigo.key(Key::V, Direction::Click);
        let release = enigo.key(Key::Control, Direction::Release);
        click.and(release)
    })();
    if let Err(error) = paste {
        return Err(AppError::new("injection.permission", error.to_string()).fallback("copy"));
    }

    std::thread::sleep(std::time::Duration::from_millis(80));
    restore_clipboard_later(text.to_owned(), previous);
    Ok(())
}

fn restore_clipboard_later(injected: String, previous: Option<String>) {
    let generation = {
        let mut state = CLIPBOARD_RESTORE
            .get_or_init(|| Mutex::new(ClipboardRestoreState::default()))
            .lock();
        if !state.captured {
            state.captured = true;
            state.original = previous;
        }
        state.generation = state.generation.wrapping_add(1);
        state.generation
    };
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(120));
        let original = {
            let mut state = CLIPBOARD_RESTORE
                .get_or_init(|| Mutex::new(ClipboardRestoreState::default()))
                .lock();
            if state.generation != generation {
                return;
            }
            state.captured = false;
            state.original.take()
        };
        let Some(original) = original else {
            return;
        };
        let Ok(mut clipboard) = arboard::Clipboard::new() else {
            return;
        };
        if clipboard.get_text().ok().as_deref() == Some(injected.as_str()) {
            let _ = clipboard.set_text(original);
        }
    });
}

fn send_backspaces(count: usize) -> Result<(), AppError> {
    let mut enigo = Enigo::new(&Settings::default())
        .map_err(|e| AppError::new("injection.init", e.to_string()))?;
    release_shift(&mut enigo)?;
    for _ in 0..count {
        enigo
            .key(Key::Backspace, Direction::Click)
            .map_err(|e| AppError::new("injection.backspace", e.to_string()))?;
    }
    Ok(())
}

fn release_shift(enigo: &mut Enigo) -> Result<(), AppError> {
    enigo
        .key(Key::Shift, Direction::Release)
        .map_err(|e| AppError::new("injection.modifier", e.to_string()))
}

fn common_prefix_len(previous: &str, current: &str) -> usize {
    previous
        .chars()
        .zip(current.chars())
        .take_while(|(left, right)| left == right)
        .count()
}

fn review_text_needs_replacement(current: &str, reviewed: &str) -> bool {
    current != reviewed
}

#[cfg(test)]
mod tests {
    use super::{common_prefix_len, review_text_needs_replacement};

    #[test]
    fn provisional_replacement_is_character_safe() {
        assert_eq!(common_prefix_len("飞书文当", "飞书文档"), 3);
    }

    #[test]
    fn committing_matching_text_does_not_replace_it() {
        assert!(!review_text_needs_replacement("已实时上屏", "已实时上屏"));
        assert!(review_text_needs_replacement("原文", "格式化结果"));
    }
}

#[cfg(windows)]
pub fn capture_foreground() -> FocusTarget {
    use windows::Win32::UI::WindowsAndMessaging::{
        GetForegroundWindow, GetWindowTextW, GetWindowThreadProcessId,
    };
    unsafe {
        let hwnd = GetForegroundWindow();
        let mut pid = 0u32;
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
        let mut title = [0u16; 512];
        let len = GetWindowTextW(hwnd, &mut title);
        FocusTarget {
            window_id: hwnd.0 as isize,
            process_id: pid,
            title: String::from_utf16_lossy(&title[..len.max(0) as usize]),
            captured_at: chrono::Utc::now().to_rfc3339(),
        }
    }
}

#[cfg(not(windows))]
pub fn capture_foreground() -> FocusTarget {
    FocusTarget {
        window_id: 0,
        process_id: 0,
        title: "Unsupported platform".into(),
        captured_at: chrono::Utc::now().to_rfc3339(),
    }
}

#[cfg(windows)]
fn is_same_target(target: &FocusTarget) -> bool {
    use windows::Win32::Foundation::HWND;
    use windows::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, IsWindow};
    unsafe {
        let expected = HWND(target.window_id as *mut core::ffi::c_void);
        IsWindow(Some(expected)).as_bool() && GetForegroundWindow() == expected
    }
}

#[cfg(not(windows))]
fn is_same_target(_target: &FocusTarget) -> bool {
    false
}
