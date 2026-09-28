use crate::error::AppError;
use crate::models::{Hotword, PromptTemplate, RecordItem};
use parking_lot::Mutex;
use rusqlite::{params, Connection};
use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

#[derive(Debug, Clone)]
pub struct ModelState {
    pub id: String,
    pub path: Option<String>,
    pub status: String,
    pub progress: u8,
}

#[derive(Clone)]
pub struct Database(pub Arc<Mutex<Connection>>);

impl Database {
    pub fn open(path: &Path) -> Result<Self, AppError> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| AppError::new("storage.create", e.to_string()))?;
        }
        let conn =
            Connection::open(path).map_err(|e| AppError::new("storage.open", e.to_string()))?;
        conn.execute_batch(
            "PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;
             CREATE TABLE IF NOT EXISTS settings(key TEXT PRIMARY KEY, value TEXT NOT NULL, updated_at TEXT NOT NULL);
             CREATE TABLE IF NOT EXISTS hotwords(id INTEGER PRIMARY KEY AUTOINCREMENT, word TEXT NOT NULL UNIQUE, category TEXT NOT NULL, behavior TEXT NOT NULL, replacement TEXT, priority INTEGER NOT NULL DEFAULT 3, apps TEXT NOT NULL DEFAULT '[\"全部应用\"]', hits INTEGER NOT NULL DEFAULT 0, enabled INTEGER NOT NULL DEFAULT 1, note TEXT NOT NULL DEFAULT '', updated_at TEXT NOT NULL);
             CREATE TABLE IF NOT EXISTS records(id TEXT PRIMARY KEY, started_at TEXT NOT NULL, app_name TEXT NOT NULL, duration_ms INTEGER NOT NULL, char_count INTEGER NOT NULL, transcript TEXT NOT NULL, formatted_text TEXT, llm_provider TEXT, llm_model TEXT, status TEXT NOT NULL, created_at TEXT NOT NULL);
             CREATE TABLE IF NOT EXISTS prompt_templates(id INTEGER PRIMARY KEY AUTOINCREMENT, name TEXT NOT NULL, trigger_word TEXT NOT NULL DEFAULT '', prompt TEXT NOT NULL, enabled INTEGER NOT NULL DEFAULT 1, is_default INTEGER NOT NULL DEFAULT 0, updated_at TEXT NOT NULL);
             CREATE TABLE IF NOT EXISTS models(id TEXT PRIMARY KEY, engine TEXT NOT NULL, name TEXT NOT NULL, path TEXT, size_bytes INTEGER, sha256 TEXT, status TEXT NOT NULL DEFAULT 'idle', progress INTEGER NOT NULL DEFAULT 0, updated_at TEXT NOT NULL);
             CREATE TABLE IF NOT EXISTS secrets(name TEXT PRIMARY KEY, value TEXT NOT NULL, updated_at TEXT NOT NULL);"
        ).map_err(|e| AppError::new("storage.migrate", e.to_string()))?;
        let _ = conn.execute("ALTER TABLE hotwords ADD COLUMN last_used TEXT", []);
        let _ = conn.execute("ALTER TABLE records ADD COLUMN llm_model TEXT", []);
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS record_hotwords(
               record_id TEXT NOT NULL,
               hotword_id INTEGER NOT NULL,
               word TEXT NOT NULL,
               hit_count INTEGER NOT NULL DEFAULT 1,
               PRIMARY KEY(record_id, hotword_id)
             );",
        )
        .map_err(|e| AppError::new("storage.migrate", e.to_string()))?;
        let _ = conn.execute(
            "ALTER TABLE record_hotwords ADD COLUMN hit_count INTEGER NOT NULL DEFAULT 1",
            [],
        );
        Ok(Self(Arc::new(Mutex::new(conn))))
    }

    pub fn snapshot_to(&self, path: &Path) -> Result<(), AppError> {
        self.0
            .lock()
            .execute("VACUUM INTO ?1", params![path.to_string_lossy().as_ref()])
            .map_err(|error| AppError::new("data.database_copy", error.to_string()))?;
        Ok(())
    }

    pub fn verify(&self) -> Result<(), AppError> {
        let result: String = self
            .0
            .lock()
            .query_row("PRAGMA quick_check", [], |row| row.get(0))
            .map_err(|error| AppError::new("data.database_verify", error.to_string()))?;
        if result == "ok" {
            Ok(())
        } else {
            Err(AppError::new("data.database_verify", result))
        }
    }

    pub fn rewrite_storage_paths(&self, old_root: &Path, new_root: &Path) -> Result<(), AppError> {
        let conn = self.0.lock();
        let mut model_paths = Vec::new();
        {
            let mut statement = conn
                .prepare("SELECT id,path FROM models WHERE path IS NOT NULL")
                .map_err(|error| AppError::new("data.database_rewrite", error.to_string()))?;
            let rows = statement
                .query_map([], |row| {
                    Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
                })
                .map_err(|error| AppError::new("data.database_rewrite", error.to_string()))?;
            for row in rows {
                model_paths.push(
                    row.map_err(|error| AppError::new("data.database_rewrite", error.to_string()))?,
                );
            }
        }
        for (id, value) in model_paths {
            if let Some(rewritten) = rewrite_path(&value, old_root, new_root) {
                conn.execute(
                    "UPDATE models SET path=?1 WHERE id=?2",
                    params![rewritten, id],
                )
                .map_err(|error| AppError::new("data.database_rewrite", error.to_string()))?;
            }
        }
        let settings: Option<String> = conn
            .query_row(
                "SELECT value FROM settings WHERE key='appSettings'",
                [],
                |row| row.get(0),
            )
            .ok();
        if let Some(settings) = settings {
            let mut value: serde_json::Value = serde_json::from_str(&settings)
                .map_err(|error| AppError::new("data.database_rewrite", error.to_string()))?;
            if let Some(path) = value.get_mut("localModelPath") {
                if let Some(current) = path.as_str() {
                    if let Some(rewritten) = rewrite_path(current, old_root, new_root) {
                        *path = serde_json::Value::String(rewritten);
                    }
                }
            }
            conn.execute(
                "UPDATE settings SET value=?1,updated_at=datetime('now') WHERE key='appSettings'",
                params![value.to_string()],
            )
            .map_err(|error| AppError::new("data.database_rewrite", error.to_string()))?;
        }
        Ok(())
    }

    pub fn set_secret(&self, name: &str, value: &str) -> Result<(), AppError> {
        self.0.lock().execute(
            "INSERT INTO secrets(name,value,updated_at) VALUES(?1,?2,datetime('now')) ON CONFLICT(name) DO UPDATE SET value=excluded.value,updated_at=excluded.updated_at",
            params![name, value],
        ).map_err(|e| AppError::new("secret.write", e.to_string()))?;
        Ok(())
    }

    pub fn get_secret(&self, name: &str) -> Result<String, AppError> {
        self.0
            .lock()
            .query_row(
                "SELECT value FROM secrets WHERE name=?1",
                params![name],
                |row| row.get(0),
            )
            .map_err(|_| AppError::new("secret.missing", "尚未配置 API Key"))
    }

    pub fn secret_configured(&self, name: &str) -> bool {
        self.get_secret(name)
            .map(|value| !value.is_empty())
            .unwrap_or(false)
    }

    pub fn set_setting(&self, key: &str, value: &str) -> Result<(), AppError> {
        self.0.lock().execute(
            "INSERT INTO settings(key,value,updated_at) VALUES(?1,?2,datetime('now')) ON CONFLICT(key) DO UPDATE SET value=excluded.value,updated_at=excluded.updated_at",
            params![key, value],
        ).map_err(|e| AppError::new("storage.write", e.to_string()))?;
        Ok(())
    }

    pub fn get_settings(&self) -> Result<serde_json::Value, AppError> {
        let conn = self.0.lock();
        let mut stmt = conn
            .prepare("SELECT key,value FROM settings")
            .map_err(|e| AppError::new("storage.read", e.to_string()))?;
        let mut out = serde_json::Map::new();
        let rows = stmt
            .query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })
            .map_err(|e| AppError::new("storage.read", e.to_string()))?;
        for row in rows {
            let (k, v) = row.map_err(|e| AppError::new("storage.read", e.to_string()))?;
            out.insert(
                k,
                serde_json::from_str(&v).unwrap_or(serde_json::Value::String(v)),
            );
        }
        Ok(serde_json::Value::Object(out))
    }

    pub fn list_hotwords(&self) -> Result<Vec<Hotword>, AppError> {
        let conn = self.0.lock();
        let mut stmt = conn.prepare(
            "SELECT id,word,category,behavior,replacement,priority,apps,hits,last_used,enabled,note FROM hotwords ORDER BY priority DESC,id DESC"
        ).map_err(|e| AppError::new("storage.read", e.to_string()))?;
        let rows = stmt
            .query_map([], |row| {
                let apps: String = row.get(6)?;
                Ok(Hotword {
                    id: row.get(0)?,
                    word: row.get(1)?,
                    category: row.get(2)?,
                    behavior: row.get(3)?,
                    replacement: row.get(4)?,
                    priority: row.get(5)?,
                    apps: serde_json::from_str(&apps).unwrap_or_else(|_| vec!["全部应用".into()]),
                    hits: row.get(7)?,
                    last_used: row.get::<_, Option<String>>(8)?.unwrap_or_default(),
                    enabled: row.get::<_, i64>(9)? != 0,
                    note: row.get(10)?,
                })
            })
            .map_err(|e| AppError::new("storage.read", e.to_string()))?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|e| AppError::new("storage.read", e.to_string()))
    }

    pub fn upsert_hotword(&self, hotword: &Hotword) -> Result<(), AppError> {
        let apps = serde_json::to_string(&hotword.apps).unwrap_or_else(|_| "[]".into());
        self.0.lock().execute(
            "INSERT INTO hotwords(id,word,category,behavior,replacement,priority,apps,hits,last_used,enabled,note,updated_at)
             VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,datetime('now'))
             ON CONFLICT(id) DO UPDATE SET word=excluded.word,category=excluded.category,behavior=excluded.behavior,replacement=excluded.replacement,priority=excluded.priority,apps=excluded.apps,hits=excluded.hits,last_used=excluded.last_used,enabled=excluded.enabled,note=excluded.note,updated_at=excluded.updated_at",
            params![hotword.id, hotword.word, hotword.category, hotword.behavior, hotword.replacement, hotword.priority, apps, hotword.hits, hotword.last_used, hotword.enabled as i64, hotword.note],
        ).map_err(|e| AppError::new("hotword.write", e.to_string()))?;
        Ok(())
    }

    pub fn delete_hotword(&self, id: i64) -> Result<(), AppError> {
        self.0
            .lock()
            .execute("DELETE FROM hotwords WHERE id=?1", params![id])
            .map_err(|e| AppError::new("hotword.delete", e.to_string()))?;
        Ok(())
    }

    pub fn insert_record(&self, record: &RecordItem) -> Result<(), AppError> {
        let conn = self.0.lock();
        conn.execute(
            "INSERT OR REPLACE INTO records(id,started_at,app_name,duration_ms,char_count,transcript,formatted_text,llm_provider,llm_model,status,created_at)
             VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,'completed',datetime('now'))",
            params![record.id, record.started_at, record.app, record.duration * 1000, record.chars as i64, record.text, record.formatted_text, record.llm_provider, record.model],
        ).map_err(|e| AppError::new("record.write", e.to_string()))?;
        let mut counts = std::collections::HashMap::<&str, i64>::new();
        for word in &record.hotwords {
            *counts.entry(word).or_default() += 1;
        }
        for (word, count) in counts {
            conn.execute(
                "INSERT OR REPLACE INTO record_hotwords(record_id,hotword_id,word,hit_count) VALUES(?1,COALESCE((SELECT id FROM hotwords WHERE word=?2),0),?2,?3)",
                params![record.id, word, count],
            ).map_err(|e| AppError::new("record.write", e.to_string()))?;
        }
        Ok(())
    }

    pub fn list_records(&self) -> Result<Vec<RecordItem>, AppError> {
        let conn = self.0.lock();
        let mut stmt = conn.prepare(
            "SELECT r.id,r.started_at,r.app_name,r.duration_ms,r.char_count,r.transcript,r.llm_provider,r.formatted_text,r.llm_model,
             COALESCE((SELECT json_group_array(json_object('word',word,'count',hit_count)) FROM record_hotwords rh WHERE rh.record_id=r.id),'[]')
             FROM records r ORDER BY r.started_at DESC LIMIT 500"
        ).map_err(|e| AppError::new("storage.read", e.to_string()))?;
        let rows = stmt
            .query_map([], |row| {
                let provider: Option<String> = row.get(6)?;
                let words: String = row.get(9)?;
                let hits: Vec<serde_json::Value> = serde_json::from_str(&words).unwrap_or_default();
                let hotwords = hits
                    .into_iter()
                    .flat_map(|hit| {
                        let word = hit
                            .get("word")
                            .and_then(|value| value.as_str())
                            .unwrap_or_default()
                            .to_owned();
                        let count = hit
                            .get("count")
                            .and_then(|value| value.as_i64())
                            .unwrap_or(1)
                            .max(0) as usize;
                        std::iter::repeat_n(word, count)
                    })
                    .collect();
                Ok(RecordItem {
                    id: row.get(0)?,
                    started_at: row.get(1)?,
                    app: row.get(2)?,
                    duration: (row.get::<_, i64>(3)?.max(0) as u64) / 1000,
                    chars: row.get::<_, i64>(4)?.max(0) as usize,
                    text: row.get(5)?,
                    ai: provider.is_some(),
                    llm_provider: provider,
                    formatted_text: row.get(7)?,
                    model: row.get(8)?,
                    hotwords,
                })
            })
            .map_err(|e| AppError::new("storage.read", e.to_string()))?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|e| AppError::new("storage.read", e.to_string()))
    }

    pub fn delete_expired_records(&self, retention_days: i64) -> Result<usize, AppError> {
        if retention_days <= 0 {
            return Ok(0);
        }
        let modifier = format!("-{retention_days} days");
        let conn = self.0.lock();
        conn.execute(
            "DELETE FROM record_hotwords WHERE record_id IN (
                SELECT id FROM records WHERE datetime(started_at) < datetime('now', ?1)
             )",
            params![modifier],
        )
        .map_err(|e| AppError::new("record.purge", e.to_string()))?;
        conn.execute(
            "DELETE FROM records WHERE datetime(started_at) < datetime('now', ?1)",
            params![modifier],
        )
        .map_err(|e| AppError::new("record.purge", e.to_string()))
    }

    pub fn clear_records(&self) -> Result<(), AppError> {
        let conn = self.0.lock();
        conn.execute("DELETE FROM record_hotwords", [])
            .map_err(|e| AppError::new("record.clear", e.to_string()))?;
        conn.execute("DELETE FROM records", [])
            .map_err(|e| AppError::new("record.clear", e.to_string()))?;
        Ok(())
    }

    pub fn list_prompts(&self) -> Result<Vec<PromptTemplate>, AppError> {
        let conn = self.0.lock();
        let mut stmt = conn
            .prepare(
                "SELECT id,name,trigger_word,prompt,enabled,is_default FROM prompt_templates ORDER BY is_default DESC,id ASC",
            )
            .map_err(|e| AppError::new("storage.read", e.to_string()))?;
        let rows = stmt
            .query_map([], |row| {
                Ok(PromptTemplate {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    trigger_word: row.get(2)?,
                    prompt: row.get(3)?,
                    enabled: row.get::<_, i64>(4)? != 0,
                    is_default: row.get::<_, i64>(5)? != 0,
                })
            })
            .map_err(|e| AppError::new("storage.read", e.to_string()))?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|e| AppError::new("storage.read", e.to_string()))
    }

    pub fn seed_prompts_if_empty(&self) -> Result<(), AppError> {
        let count: i64 = self
            .0
            .lock()
            .query_row("SELECT COUNT(*) FROM prompt_templates", [], |row| row.get(0))
            .map_err(|e| AppError::new("storage.read", e.to_string()))?;
        if count > 0 {
            return Ok(());
        }
        let seeds = [
            (
                "会议纪要整理",
                "纪要",
                "你是会议纪要助手。将用户的口语转写整理为结构化纪要：先给一句标题，再分要点列出结论与待办。保留所有数字与专有名词。\n可用变量：{{text}}、{{hotwords}}、{{datetime}}。",
                1,
            ),
            (
                "口语转书面",
                "书面",
                "把口语转写改写为书面语：删除语气词，修正口误，合并重复表达；不改变原意，不新增信息。\n变量：{{text}}。",
                0,
            ),
            (
                "邮件润色",
                "邮件",
                "将转写内容整理成一封简洁的中文工作邮件：关键信息前置，正文分 2-3 段。\n变量：{{text}}、{{datetime}}。",
                0,
            ),
        ];
        for (name, trigger, prompt, is_default) in seeds {
            self.0.lock().execute(
                "INSERT INTO prompt_templates(name,trigger_word,prompt,enabled,is_default,updated_at) VALUES(?1,?2,?3,1,?4,datetime('now'))",
                params![name, trigger, prompt, is_default],
            ).map_err(|e| AppError::new("prompt.seed", e.to_string()))?;
        }
        Ok(())
    }

    pub fn upsert_prompt(&self, prompt: &PromptTemplate) -> Result<PromptTemplate, AppError> {
        let conn = self.0.lock();
        if prompt.is_default {
            conn.execute("UPDATE prompt_templates SET is_default=0", [])
                .map_err(|e| AppError::new("prompt.write", e.to_string()))?;
        }
        if prompt.id > 0 {
            conn.execute(
                "UPDATE prompt_templates SET name=?2,trigger_word=?3,prompt=?4,enabled=?5,is_default=?6,updated_at=datetime('now') WHERE id=?1",
                params![prompt.id, prompt.name, prompt.trigger_word, prompt.prompt, prompt.enabled as i64, prompt.is_default as i64],
            ).map_err(|e| AppError::new("prompt.write", e.to_string()))?;
            return Ok(prompt.clone());
        }
        conn.execute(
            "INSERT INTO prompt_templates(name,trigger_word,prompt,enabled,is_default,updated_at) VALUES(?1,?2,?3,?4,?5,datetime('now'))",
            params![prompt.name, prompt.trigger_word, prompt.prompt, prompt.enabled as i64, prompt.is_default as i64],
        )
        .map_err(|e| AppError::new("prompt.write", e.to_string()))?;
        let id = conn.last_insert_rowid();
        Ok(PromptTemplate {
            id,
            ..prompt.clone()
        })
    }

    pub fn delete_prompt(&self, id: i64) -> Result<(), AppError> {
        self.0
            .lock()
            .execute("DELETE FROM prompt_templates WHERE id=?1", params![id])
            .map_err(|e| AppError::new("prompt.delete", e.to_string()))?;
        Ok(())
    }

    pub fn list_models(&self) -> Result<Vec<ModelState>, AppError> {
        let conn = self.0.lock();
        let mut stmt = conn
            .prepare("SELECT id,path,status,progress FROM models")
            .map_err(|e| AppError::new("storage.read", e.to_string()))?;
        let rows = stmt
            .query_map([], |row| {
                Ok(ModelState {
                    id: row.get(0)?,
                    path: row.get(1)?,
                    status: row.get(2)?,
                    progress: row.get::<_, i64>(3)?.clamp(0, 100) as u8,
                })
            })
            .map_err(|e| AppError::new("storage.read", e.to_string()))?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|e| AppError::new("storage.read", e.to_string()))
    }

    pub fn set_model_state(
        &self,
        id: &str,
        engine: &str,
        name: &str,
        path: Option<&str>,
        status: &str,
        progress: u8,
    ) -> Result<(), AppError> {
        self.0.lock().execute(
            "INSERT INTO models(id,engine,name,path,status,progress,updated_at) VALUES(?1,?2,?3,?4,?5,?6,datetime('now')) ON CONFLICT(id) DO UPDATE SET path=excluded.path,status=excluded.status,progress=excluded.progress,updated_at=excluded.updated_at",
            params![id, engine, name, path, status, progress],
        ).map_err(|e| AppError::new("storage.write", e.to_string()))?;
        Ok(())
    }

    pub fn recover_interrupted_model_downloads(&self) -> Result<(), AppError> {
        self.0
            .lock()
            .execute(
                "UPDATE models SET status='paused',updated_at=datetime('now') WHERE status='downloading'",
                [],
            )
            .map_err(|e| AppError::new("storage.write", e.to_string()))?;
        Ok(())
    }

    pub fn delete_model_state(&self, id: &str) -> Result<(), AppError> {
        self.0
            .lock()
            .execute("DELETE FROM models WHERE id=?1", params![id])
            .map_err(|e| AppError::new("storage.write", e.to_string()))?;
        Ok(())
    }
}

fn rewrite_path(value: &str, old_root: &Path, new_root: &Path) -> Option<String> {
    let relative = PathBuf::from(value)
        .strip_prefix(old_root)
        .ok()?
        .to_path_buf();
    Some(new_root.join(relative).to_string_lossy().into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn in_memory_database() -> Database {
        let connection = Connection::open_in_memory().expect("open in-memory database");
        connection
            .execute_batch(
                "CREATE TABLE models(
                    id TEXT PRIMARY KEY,
                    engine TEXT NOT NULL,
                    name TEXT NOT NULL,
                    path TEXT,
                    size_bytes INTEGER,
                    sha256 TEXT,
                    status TEXT NOT NULL DEFAULT 'idle',
                    progress INTEGER NOT NULL DEFAULT 0,
                    updated_at TEXT NOT NULL
                );
                CREATE TABLE settings(key TEXT PRIMARY KEY,value TEXT NOT NULL,updated_at TEXT NOT NULL);
                CREATE TABLE hotwords(id INTEGER PRIMARY KEY,word TEXT NOT NULL UNIQUE,category TEXT NOT NULL,behavior TEXT NOT NULL,replacement TEXT,priority INTEGER NOT NULL,apps TEXT NOT NULL,hits INTEGER NOT NULL,last_used TEXT,enabled INTEGER NOT NULL,note TEXT NOT NULL,updated_at TEXT NOT NULL);
                CREATE TABLE records(id TEXT PRIMARY KEY,started_at TEXT NOT NULL,app_name TEXT NOT NULL,duration_ms INTEGER NOT NULL,char_count INTEGER NOT NULL,transcript TEXT NOT NULL,formatted_text TEXT,llm_provider TEXT,llm_model TEXT,status TEXT NOT NULL,created_at TEXT NOT NULL);
                CREATE TABLE record_hotwords(record_id TEXT NOT NULL,hotword_id INTEGER NOT NULL,word TEXT NOT NULL,hit_count INTEGER NOT NULL DEFAULT 1,PRIMARY KEY(record_id,hotword_id));
                CREATE TABLE prompt_templates(id INTEGER PRIMARY KEY AUTOINCREMENT, name TEXT NOT NULL, trigger_word TEXT NOT NULL DEFAULT '', prompt TEXT NOT NULL, enabled INTEGER NOT NULL DEFAULT 1, is_default INTEGER NOT NULL DEFAULT 0, updated_at TEXT NOT NULL);",
            )
            .expect("create models table");
        Database(Arc::new(Mutex::new(connection)))
    }

    #[test]
    fn interrupted_downloads_are_recovered_as_paused() {
        let db = in_memory_database();
        db.set_model_state(
            "interrupted",
            "funasr",
            "Interrupted",
            Some("models/interrupted"),
            "downloading",
            42,
        )
        .unwrap();
        db.set_model_state(
            "complete",
            "funasr",
            "Complete",
            Some("models/complete"),
            "done",
            100,
        )
        .unwrap();

        db.recover_interrupted_model_downloads().unwrap();

        let states = db.list_models().unwrap();
        let interrupted = states
            .iter()
            .find(|state| state.id == "interrupted")
            .unwrap();
        let complete = states.iter().find(|state| state.id == "complete").unwrap();
        assert_eq!(interrupted.status, "paused");
        assert_eq!(interrupted.progress, 42);
        assert_eq!(complete.status, "done");
        assert_eq!(complete.progress, 100);
    }

    #[test]
    fn hotwords_and_records_round_trip() {
        let db = in_memory_database();
        let hotword = Hotword {
            id: 7,
            word: "语流".into(),
            category: "brand".into(),
            behavior: "仅统计，不强改写".into(),
            replacement: None,
            priority: 3,
            apps: vec!["全部应用".into()],
            hits: 0,
            last_used: String::new(),
            enabled: true,
            note: "品牌词".into(),
        };
        db.upsert_hotword(&hotword).unwrap();
        assert_eq!(db.list_hotwords().unwrap()[0].word, "语流");

        let record = RecordItem {
            id: "session-1".into(),
            started_at: "2026-09-05T01:00:00Z".into(),
            app: "测试窗口".into(),
            duration: 12,
            chars: 2,
            hotwords: vec!["语流".into(), "语流".into()],
            ai: true,
            llm_provider: Some("openai".into()),
            model: Some("gpt-5-mini".into()),
            text: "语流".into(),
            formatted_text: Some("整理后的语流".into()),
        };
        db.insert_record(&record).unwrap();
        let records = db.list_records().unwrap();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].hotwords, vec!["语流", "语流"]);
        assert_eq!(records[0].duration, 12);
        assert_eq!(records[0].text, "语流");
        assert_eq!(records[0].formatted_text.as_deref(), Some("整理后的语流"));
        assert_eq!(records[0].llm_provider.as_deref(), Some("openai"));
        assert_eq!(records[0].model.as_deref(), Some("gpt-5-mini"));

        let mut counted = hotword.clone();
        counted.hits = 4;
        counted.last_used = "2026-09-11 12:00:00".into();
        db.upsert_hotword(&counted).unwrap();
        assert_eq!(db.list_hotwords().unwrap()[0].hits, 4);
        assert_eq!(db.list_hotwords().unwrap()[0].last_used, "2026-09-11 12:00:00");
    }

    #[test]
    fn prompt_templates_seed_once_and_keep_single_default() {
        let db = in_memory_database();
        db.seed_prompts_if_empty().unwrap();
        db.seed_prompts_if_empty().unwrap();
        assert_eq!(db.list_prompts().unwrap().len(), 3);

        let updated = db
            .upsert_prompt(&PromptTemplate {
                id: 0,
                name: "待办提取".into(),
                trigger_word: "待办".into(),
                prompt: "只提取待办".into(),
                enabled: true,
                is_default: true,
            })
            .unwrap();
        assert!(updated.id > 0);
        let defaults = db
            .list_prompts()
            .unwrap()
            .into_iter()
            .filter(|item| item.is_default)
            .count();
        assert_eq!(defaults, 1);
    }

    #[test]
    fn expired_records_are_deleted_by_retention() {
        let db = in_memory_database();
        let old = RecordItem {
            id: "old".into(),
            started_at: "2020-01-01T00:00:00Z".into(),
            app: "测试".into(),
            duration: 1,
            chars: 1,
            hotwords: vec![],
            ai: false,
            llm_provider: None,
            model: None,
            text: "旧".into(),
            formatted_text: None,
        };
        let recent = RecordItem {
            id: "new".into(),
            started_at: chrono::Utc::now().to_rfc3339(),
            app: "测试".into(),
            duration: 1,
            chars: 1,
            hotwords: vec![],
            ai: false,
            llm_provider: None,
            model: None,
            text: "新".into(),
            formatted_text: None,
        };
        db.insert_record(&old).unwrap();
        db.insert_record(&recent).unwrap();
        db.delete_expired_records(30).unwrap();
        let records = db.list_records().unwrap();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].id, "new");
    }

    #[test]
    fn app_settings_round_trip_silence_auto_end() {
        let db = in_memory_database();
        db.set_setting(
            "appSettings",
            r#"{"liveInsert":true,"autoEndOnSilence":false}"#,
        )
        .unwrap();
        let saved = db.get_settings().unwrap();
        assert_eq!(saved["appSettings"]["autoEndOnSilence"], false);
    }
}
