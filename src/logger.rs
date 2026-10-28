use crate::config::LogLevel;
use chrono::Local;

pub struct Logger {
    level: LogLevel,
}

impl Logger {
    pub fn new(level: LogLevel) -> Self {
        Logger { level }
    }

    pub fn enabled(&self, level: LogLevel) -> bool {
        level >= self.level
    }

    pub fn log(&self, level: LogLevel, message: &str, fields: &[(&str, String)]) {
        if !self.enabled(level) {
            return;
        }
        let mut line = format!(
            "{} {} {}",
            Local::now().format("%Y-%m-%dT%H:%M:%S%z"),
            level.as_str().to_uppercase(),
            message
        );
        for (key, value) in fields {
            line.push(' ');
            line.push_str(key);
            line.push('=');
            line.push_str(value);
        }
        eprintln!("{line}");
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
