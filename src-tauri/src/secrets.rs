use crate::{db::Database, error::AppError};

pub fn set(db: &Database, name: &str, value: &str) -> Result<(), AppError> {
    db.set_secret(name, value)
}

pub fn get(db: &Database, name: &str) -> Result<String, AppError> {
    db.get_secret(name)
}

pub fn configured(db: &Database, name: &str) -> bool {
    db.secret_configured(name)
}
