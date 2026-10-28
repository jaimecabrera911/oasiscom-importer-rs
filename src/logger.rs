use crate::config::LogLevel;
use chrono::Local;
use std::fs::{File, OpenOptions};
use std::io::{IsTerminal, Write};
use std::path::Path;
use std::sync::Mutex;

pub struct Logger {
    level: LogLevel,
    file: Option<Mutex<File>>,
    styled: bool,
}

impl Logger {
    pub fn new(level: LogLevel, log_file: &Path) -> Self {
        let file = if log_file.as_os_str().is_empty() {
            None
        } else {
            match open_log_file(log_file) {
                Ok(f) => Some(Mutex::new(f)),
                Err(e) => {
                    eprintln!(
                        "advertencia: no se pudo abrir LOG_FILE {}: {e}",
                        log_file.display()
                    );
                    None
                }
            }
        };
        let styled = std::io::stderr().is_terminal() && std::env::var_os("NO_COLOR").is_none();
        Logger {
            level,
            file,
            styled,
        }
    }

    pub fn enabled(&self, level: LogLevel) -> bool {
        level >= self.level
    }

    pub fn log(&self, level: LogLevel, message: &str, fields: &[(&str, String)]) {
        if !self.enabled(level) {
            return;
        }
        let timestamp = Local::now().format("%Y-%m-%dT%H:%M:%S%z");
        let mut suffix = String::new();
        for (key, value) in fields {
            suffix.push(' ');
            suffix.push_str(key);
            suffix.push('=');
            suffix.push_str(value);
        }
        let name = level.as_str().to_uppercase();
        let plain = format!("{timestamp} {name} {message}{suffix}");

        if self.styled {
            let (color, emoji) = style(level);
            let colored = format!("\x1b[{color}m{emoji} {name}\x1b[0m");
            eprintln!("{timestamp} {colored} {message}{suffix}");
        } else {
            eprintln!("{plain}");
        }

        if let Some(file) = &self.file {
            if let Ok(mut file) = file.lock() {
                let _ = writeln!(file, "{plain}");
                let _ = file.flush();
            }
        }
    }

    pub fn debug(&self, message: &str, fields: &[(&str, String)]) {
        self.log(LogLevel::Debug, message, fields);
    }

    pub fn info(&self, message: &str, fields: &[(&str, String)]) {
        self.log(LogLevel::Info, message, fields);
    }

    pub fn warn(&self, message: &str, fields: &[(&str, String)]) {
        self.log(LogLevel::Warn, message, fields);
    }

    pub fn error(&self, message: &str, fields: &[(&str, String)]) {
        self.log(LogLevel::Error, message, fields);
    }
}

fn style(level: LogLevel) -> (&'static str, &'static str) {
    match level {
        LogLevel::Debug => ("90", "🔍"),
        LogLevel::Info => ("32", "✅"),
        LogLevel::Warn => ("33", "⚠️"),
        LogLevel::Error => ("31", "❌"),
    }
}

fn open_log_file(path: &Path) -> std::io::Result<File> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)?;
        }
    }
    OpenOptions::new().create(true).append(true).open(path)
}

pub fn mask_context(context: &str) -> String {
    if context.is_empty() {
        return "(sin configurar)".to_string();
    }
    if context.chars().count() <= 4 {
        return "***".to_string();
    }
    let chars: Vec<char> = context.chars().collect();
    let first: String = chars[..2].iter().collect();
    let last: String = chars[chars.len() - 2..].iter().collect();
    format!("{first}***{last}")
}
