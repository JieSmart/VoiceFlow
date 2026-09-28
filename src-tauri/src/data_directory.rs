use crate::{db::Database, error::AppError};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::{Path, PathBuf},
};
use uuid::Uuid;

const LOCATION_VERSION: u8 = 1;
const DATA_FOLDER_NAME: &str = "语流";

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DataDirectoryInfo {
    pub current_path: String,
    pub default_path: String,
    pub is_custom: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DataDirectoryChoice {
    pub parent_path: String,
    pub target_path: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DataMigrationResult {
    pub old_path: String,
    pub new_path: String,
    pub restart_required: bool,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LocationFile {
    version: u8,
    data_root: PathBuf,
}

pub fn resolve(default_root: &Path, location_file: &Path) -> Result<PathBuf, AppError> {
    if !location_file.exists() {
        return Ok(default_root.to_path_buf());
    }
    let bytes = fs::read(location_file)
        .map_err(|error| AppError::new("data.location_read", error.to_string()))?;
    let location: LocationFile = serde_json::from_slice(&bytes)
        .map_err(|error| AppError::new("data.location_invalid", error.to_string()))?;
    if location.version != LOCATION_VERSION || !location.data_root.is_absolute() {
        return Err(AppError::new(
            "data.location_invalid",
            "应用数据目录配置无效，请检查 data-location.json",
        ));
    }
    if !location.data_root.join("yuliu.sqlite3").is_file() {
        return Err(AppError::new(
            "data.location_missing",
            format!("应用数据目录不可用：{}", location.data_root.display()),
        ));
    }
    Ok(location.data_root)
}

pub fn info(current_root: &Path, default_root: &Path) -> DataDirectoryInfo {
    DataDirectoryInfo {
        current_path: current_root.to_string_lossy().into_owned(),
        default_path: default_root.to_string_lossy().into_owned(),
        is_custom: current_root != default_root,
    }
}

pub fn choice(parent: PathBuf) -> Result<DataDirectoryChoice, AppError> {
    if !parent.is_absolute() || !parent.is_dir() {
        return Err(AppError::new(
            "data.parent_invalid",
            "请选择有效的本地文件夹",
        ));
    }
    fs::canonicalize(&parent)
        .map_err(|error| AppError::new("data.parent_invalid", error.to_string()))?;
    let target = parent.join(DATA_FOLDER_NAME);
    Ok(DataDirectoryChoice {
        parent_path: parent.to_string_lossy().into_owned(),
        target_path: target.to_string_lossy().into_owned(),
    })
}

pub fn migrate(
    db: &Database,
    current_root: &Path,
    default_root: &Path,
    location_file: &Path,
    parent: &Path,
) -> Result<DataMigrationResult, AppError> {
    let selected = choice(parent.to_path_buf())?;
    let target = PathBuf::from(&selected.target_path);
    validate_target(current_root, &target)?;

    let target_existed = target.exists();
    if target_existed
        && fs::symlink_metadata(&target)
            .map_err(|error| AppError::new("data.target_read", error.to_string()))?
            .file_type()
            .is_symlink()
    {
        return Err(AppError::new(
            "data.target_symlink",
            "目标数据目录不能是符号链接",
        ));
    }
    if target_existed
        && fs::read_dir(&target)
            .map_err(|error| AppError::new("data.target_read", error.to_string()))?
            .next()
            .is_some()
    {
        return Err(AppError::new(
            "data.target_not_empty",
            format!("目标数据目录不是空目录：{}", target.display()),
        ));
    }

    let staging = parent.join(format!(".{DATA_FOLDER_NAME}-migrating-{}", Uuid::new_v4()));
    fs::create_dir(&staging)
        .map_err(|error| AppError::new("data.target_create", error.to_string()))?;

    let migration = (|| {
        let snapshot = staging.join("yuliu.sqlite3");
        db.snapshot_to(&snapshot)?;
        for directory in ["models", "runtime", "cache"] {
            let source = current_root.join(directory);
            if source.exists() {
                copy_directory(&source, &staging.join(directory))?;
            }
        }
        let migrated = Database::open(&snapshot)?;
        migrated.rewrite_storage_paths(current_root, &target)?;
        migrated.verify()?;
        drop(migrated);
        verify_copy(current_root, &staging)?;

        if target_existed {
            fs::remove_dir(&target)
                .map_err(|error| AppError::new("data.target_prepare", error.to_string()))?;
        }
        fs::rename(&staging, &target)
            .map_err(|error| AppError::new("data.target_commit", error.to_string()))?;
        write_location(default_root, location_file, &target)?;
        Ok(())
    })();

    if let Err(error) = migration {
        if staging.exists() {
            let _ = fs::remove_dir_all(&staging);
        }
        if target.exists() && target.join("yuliu.sqlite3").is_file() {
            let _ = fs::remove_dir_all(&target);
        }
        if target_existed && !target.exists() {
            let _ = fs::create_dir(&target);
        }
        return Err(error);
    }

    Ok(DataMigrationResult {
        old_path: current_root.to_string_lossy().into_owned(),
        new_path: target.to_string_lossy().into_owned(),
        restart_required: true,
    })
}

fn validate_target(current_root: &Path, target: &Path) -> Result<(), AppError> {
    if target.file_name().and_then(|name| name.to_str()) != Some(DATA_FOLDER_NAME) {
        return Err(AppError::new(
            "data.target_invalid",
            "目标目录必须使用“语流”子目录",
        ));
    }
    let current_for_compare =
        fs::canonicalize(current_root).unwrap_or_else(|_| current_root.to_path_buf());
    let target_for_compare = target
        .parent()
        .and_then(|parent| fs::canonicalize(parent).ok())
        .map(|parent| parent.join(DATA_FOLDER_NAME))
        .unwrap_or_else(|| target.to_path_buf());
    if target_for_compare == current_for_compare {
        return Err(AppError::new(
            "data.target_same",
            "所选位置已经是当前数据目录",
        ));
    }
    if target_for_compare.starts_with(&current_for_compare) {
        return Err(AppError::new(
            "data.target_nested",
            "新数据目录不能位于当前数据目录内部",
        ));
    }
    Ok(())
}

fn write_location(
    default_root: &Path,
    location_file: &Path,
    target: &Path,
) -> Result<(), AppError> {
    if let Some(parent) = location_file.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| AppError::new("data.location_create", error.to_string()))?;
    }
    if target == default_root {
        if location_file.exists() {
            fs::remove_file(location_file)
                .map_err(|error| AppError::new("data.location_write", error.to_string()))?;
        }
        return Ok(());
    }
    let temporary = location_file.with_extension("json.tmp");
    let value = serde_json::to_vec_pretty(&LocationFile {
        version: LOCATION_VERSION,
        data_root: target.to_path_buf(),
    })
    .map_err(|error| AppError::new("data.location_write", error.to_string()))?;
    fs::write(&temporary, value)
        .map_err(|error| AppError::new("data.location_write", error.to_string()))?;
    let backup = location_file.with_extension("json.bak");
    let had_previous = location_file.exists();
    if had_previous {
        let _ = fs::remove_file(&backup);
        fs::rename(location_file, &backup)
            .map_err(|error| AppError::new("data.location_write", error.to_string()))?;
    }
    if let Err(error) = fs::rename(&temporary, location_file) {
        if had_previous {
            let _ = fs::rename(&backup, location_file);
        }
        let _ = fs::remove_file(&temporary);
        return Err(AppError::new("data.location_write", error.to_string()));
    }
    if had_previous {
        let _ = fs::remove_file(backup);
    }
    Ok(())
}

fn copy_directory(source: &Path, destination: &Path) -> Result<(), AppError> {
    fs::create_dir(destination).map_err(|error| AppError::new("data.copy", error.to_string()))?;
    for entry in
        fs::read_dir(source).map_err(|error| AppError::new("data.copy", error.to_string()))?
    {
        let entry = entry.map_err(|error| AppError::new("data.copy", error.to_string()))?;
        let kind = entry
            .file_type()
            .map_err(|error| AppError::new("data.copy", error.to_string()))?;
        let destination = destination.join(entry.file_name());
        if kind.is_symlink() {
            return Err(AppError::new(
                "data.copy_symlink",
                "数据目录中包含不支持的符号链接",
            ));
        }
        if kind.is_dir() {
            copy_directory(&entry.path(), &destination)?;
        } else if kind.is_file() {
            fs::copy(entry.path(), destination)
                .map_err(|error| AppError::new("data.copy", error.to_string()))?;
        }
    }
    Ok(())
}

fn verify_copy(source_root: &Path, staging_root: &Path) -> Result<(), AppError> {
    for directory in ["models", "runtime", "cache"] {
        let source = source_root.join(directory);
        if source.exists() {
            verify_directory(&source, &staging_root.join(directory))?;
        }
    }
    Ok(())
}

fn verify_directory(source: &Path, destination: &Path) -> Result<(), AppError> {
    for entry in
        fs::read_dir(source).map_err(|error| AppError::new("data.verify", error.to_string()))?
    {
        let entry = entry.map_err(|error| AppError::new("data.verify", error.to_string()))?;
        let source_path = entry.path();
        let destination_path = destination.join(entry.file_name());
        let metadata = entry
            .metadata()
            .map_err(|error| AppError::new("data.verify", error.to_string()))?;
        if metadata.is_dir() {
            if !destination_path.is_dir() {
                return Err(AppError::new("data.verify", "迁移后的目录结构不完整"));
            }
            verify_directory(&source_path, &destination_path)?;
        } else {
            let copied = fs::metadata(&destination_path)
                .map_err(|error| AppError::new("data.verify", error.to_string()))?;
            if copied.len() != metadata.len() {
                return Err(AppError::new("data.verify", "迁移后的文件大小不一致"));
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_location_uses_default_root() {
        let temp = tempfile::tempdir().unwrap();
        let default = temp.path().join("default");
        assert_eq!(
            resolve(&default, &temp.path().join("missing.json")).unwrap(),
            default
        );
    }

    #[test]
    fn rejects_non_empty_and_nested_targets() {
        let temp = tempfile::tempdir().unwrap();
        let current = temp.path().join("current");
        fs::create_dir_all(current.join(DATA_FOLDER_NAME)).unwrap();
        assert_eq!(
            validate_target(&current, &current.join(DATA_FOLDER_NAME))
                .unwrap_err()
                .code,
            "data.target_nested"
        );
    }

    #[test]
    fn corrupt_location_file_is_not_silently_ignored() {
        let temp = tempfile::tempdir().unwrap();
        let location = temp.path().join("data-location.json");
        fs::write(&location, b"not-json").unwrap();
        assert_eq!(
            resolve(&temp.path().join("default"), &location)
                .unwrap_err()
                .code,
            "data.location_invalid"
        );
    }

    #[test]
    fn migration_copies_data_rewrites_paths_and_switches_location() {
        let temp = tempfile::tempdir().unwrap();
        let current = temp.path().join("default");
        let parent = temp.path().join("chosen");
        let location = temp.path().join("config").join("data-location.json");
        fs::create_dir_all(current.join("models").join("sample")).unwrap();
        fs::create_dir_all(current.join("runtime")).unwrap();
        fs::create_dir_all(current.join("cache").join("segments")).unwrap();
        fs::create_dir(&parent).unwrap();
        fs::write(
            current.join("models").join("sample").join("model.bin"),
            b"model",
        )
        .unwrap();
        fs::write(current.join("runtime").join("runtime.dll"), b"runtime").unwrap();
        fs::write(
            current.join("cache").join("segments").join("orphan.wav"),
            b"audio",
        )
        .unwrap();

        let db = Database::open(&current.join("yuliu.sqlite3")).unwrap();
        db.set_secret("asr-openai", "secret-value").unwrap();
        db.set_setting(
            "appSettings",
            &serde_json::json!({
                "localModelPath": current.join("models").join("sample").to_string_lossy()
            })
            .to_string(),
        )
        .unwrap();
        db.set_model_state(
            "sample",
            "faster-whisper",
            "Sample",
            Some(&current.join("models").join("sample").to_string_lossy()),
            "done",
            100,
        )
        .unwrap();

        let result = migrate(&db, &current, &current, &location, &parent).unwrap();
        let target = parent.join(DATA_FOLDER_NAME);
        assert_eq!(PathBuf::from(result.new_path), target);
        assert!(current.join("yuliu.sqlite3").is_file());
        assert!(target
            .join("models")
            .join("sample")
            .join("model.bin")
            .is_file());
        assert!(target.join("runtime").join("runtime.dll").is_file());
        assert!(target
            .join("cache")
            .join("segments")
            .join("orphan.wav")
            .is_file());
        assert_eq!(resolve(&current, &location).unwrap(), target);

        let migrated = Database::open(&target.join("yuliu.sqlite3")).unwrap();
        assert_eq!(migrated.get_secret("asr-openai").unwrap(), "secret-value");
        assert_eq!(
            migrated.get_settings().unwrap()["appSettings"]["localModelPath"],
            target
                .join("models")
                .join("sample")
                .to_string_lossy()
                .as_ref()
        );
        assert_eq!(
            migrated.list_models().unwrap()[0].path.as_deref(),
            Some(
                target
                    .join("models")
                    .join("sample")
                    .to_string_lossy()
                    .as_ref()
            )
        );
    }

    #[test]
    fn migration_rejects_a_non_empty_target_without_switching() {
        let temp = tempfile::tempdir().unwrap();
        let current = temp.path().join("default");
        let parent = temp.path().join("chosen");
        let target = parent.join(DATA_FOLDER_NAME);
        fs::create_dir_all(&target).unwrap();
        fs::write(target.join("existing.txt"), b"keep").unwrap();
        let db = Database::open(&current.join("yuliu.sqlite3")).unwrap();
        let location = temp.path().join("data-location.json");

        let error = migrate(&db, &current, &current, &location, &parent).unwrap_err();
        assert_eq!(error.code, "data.target_not_empty");
        assert!(!location.exists());
        assert_eq!(fs::read(target.join("existing.txt")).unwrap(), b"keep");
    }
}
