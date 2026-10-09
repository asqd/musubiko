use std::path::PathBuf;

pub type Result<T> = std::result::Result<T, Error>;

/// Errors are worded for the person using the app, in Russian, like the rest of the UI.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Не найден git версии {min} или новее. Найдены: {found}. Установите свежий git: {hint}", hint = crate::git::INSTALL_HINT, min = crate::git::fmt_version(crate::MIN_VERSION), found = if found.is_empty() { "ничего".to_string() } else { found.join(", ") })]
    GitNotFound { found: Vec<String> },

    /// A check before or after a step said no; the message says why.
    #[error("{0}")]
    Refused(String),

    #[error("Нет ветки {0}")]
    NoBranch(String),

    #[error("{0} — не git-репозиторий")]
    NotARepo(PathBuf),

    #[error("git {args} завершился с ошибкой: {stderr}")]
    Command { args: String, stderr: String },

    #[error("Не удалось запустить git: {0}")]
    Io(#[from] std::io::Error),
}
