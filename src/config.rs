use std::collections::HashMap;
use std::fmt;
use std::path::{Path, PathBuf};
use std::time::Duration;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RequestFormat {
    Multipart,
    Urlencoded,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum LogLevel {
    Debug,
    Info,
    Warn,
    Error,
}

impl LogLevel {
    pub fn as_str(self) -> &'static str {
        match self {
            LogLevel::Debug => "debug",
            LogLevel::Info => "info",
            LogLevel::Warn => "warn",
            LogLevel::Error => "error",
        }
    }
}

#[derive(Debug, Clone)]
pub struct Config {
    pub is_mock: bool,
    pub mock_latency: Duration,
    pub mock_fail: bool,

    pub token_url: String,
    pub context: String,
    pub username: String,
    pub password: String,
    pub client_id: String,
    pub client_secret: String,
    pub scope: String,
    pub grant_type: String,

    pub api_url: String,
    pub company_code: String,
    pub publication_code: String,
    pub request_format: RequestFormat,
    pub token_field: String,
    pub file_field: String,

    pub trigger_watch: bool,
    pub watch_dir: PathBuf,
    pub poll_interval: Duration,
    pub process_existing: bool,
    pub move_after_process: bool,
    pub processed_dir: PathBuf,
    pub failed_dir: PathBuf,
    pub stable_checks: u32,
    pub max_file_bytes: u64,

    pub status_enabled: bool,
    pub status_file: PathBuf,

    pub http_timeout: Duration,
    pub http_retries: u32,
    pub retry_backoff: Duration,
    pub log_level: LogLevel,
}

#[derive(Debug, Default)]
pub struct ConfigError {
    pub messages: Vec<String>,
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "configuración inválida:")?;
        for m in &self.messages {
            writeln!(f, "  - {m}")?;
        }
        Ok(())
    }
}

impl std::error::Error for ConfigError {}

impl Config {
    pub fn load() -> Result<Config, ConfigError> {
        let file = find_env_file()
            .map(|p| parse_env_file(&p))
            .unwrap_or_default();
        let env: HashMap<String, String> = std::env::vars().collect();
        Self::from_sources(&env, &file)
    }

    pub fn from_sources(
        env: &HashMap<String, String>,
        file: &HashMap<String, String>,
    ) -> Result<Config, ConfigError> {
        let raw = |key: &str| -> Option<String> {
            env.get(key)
                .or_else(|| file.get(key))
                .map(|v| v.trim().to_string())
        };
        let text = |key: &str, default: &str| -> String {
            match raw(key) {
                Some(v) if !v.is_empty() => v,
                Some(_) => String::new(),
                None => default.to_string(),
            }
        };
        let mut errors = Vec::new();

        let bool_of = |key: &str, default: bool, errors: &mut Vec<String>| -> bool {
            match raw(key) {
                None => default,
                Some(v) if v.is_empty() => default,
                Some(v) => match parse_bool(&v) {
                    Some(b) => b,
                    None => {
                        errors.push(format!("{key} tiene un valor booleano inválido: '{v}'"));
                        default
                    }
                },
            }
        };
        let int_of = |key: &str, default: i64, errors: &mut Vec<String>| -> i64 {
            match raw(key) {
                None => default,
                Some(v) if v.is_empty() => default,
                Some(v) => match v.parse::<i64>() {
                    Ok(n) => n,
                    Err(_) => {
                        errors.push(format!("{key} tiene un entero inválido: '{v}'"));
                        default
                    }
                },
            }
        };
        let dur_of = |key: &str, default_ms: u64, errors: &mut Vec<String>| -> Duration {
            match raw(key) {
                None => Duration::from_millis(default_ms),
                Some(v) if v.is_empty() => Duration::from_millis(default_ms),
                Some(v) => match parse_duration(&v) {
                    Some(d) => d,
                    None => {
                        errors.push(format!("{key} tiene una duración inválida: '{v}'"));
                        Duration::from_millis(default_ms)
                    }
                },
            }
        };

        let is_mock = bool_of("IS_MOCK", false, &mut errors);
        let mock_latency = dur_of("MOCK_LATENCY", 300, &mut errors);
        let mock_fail = bool_of("MOCK_FAIL", false, &mut errors);

        let token_url = text(
            "OASIS_TOKEN_URL",
            "https://identity.oasiscom.com/connect/token",
        );
        let context = text("OASIS_CONTEXT", "");
        let username = text("OASIS_USERNAME", "");
        let password = text("OASIS_PASSWORD", "");
        let client_id = text("OASIS_CLIENT_ID", "76db5426-2831-4105-9012-209119e53b5e");
        let client_secret = text(
            "OASIS_CLIENT_SECRET",
            "511536EF-F270-4058-80CA-1C89C192F69A",
        );
        let scope = text("OASIS_SCOPE", "OasisCo");
        let grant_type = text("OASIS_GRANT_TYPE", "password");

        let api_url = text(
            "OASIS_API_URL",
            "https://app.oasiscom.com/api/externalservices",
        );
        let company_code = text("OASIS_COMPANY_CODE", "");
        let publication_code = text("OASIS_PUBLICATION_CODE", "");
        let request_format_raw = text("OASIS_REQUEST_FORMAT", "multipart");
        let request_format = match request_format_raw.to_lowercase().as_str() {
            "multipart" => RequestFormat::Multipart,
            "urlencoded" => RequestFormat::Urlencoded,
            _ => {
                errors.push(format!(
                    "OASIS_REQUEST_FORMAT debe ser 'multipart' o 'urlencoded' (recibido: '{request_format_raw}')"
                ));
                RequestFormat::Multipart
            }
        };
        let token_field = text("OASIS_TOKEN_FIELD", "token");
        let file_field = text("OASIS_FILE_FIELD", "FileBase64");

        let trigger_watch = bool_of("TRIGGER_WATCH", true, &mut errors);
        let watch_dir_raw = text("WATCH_DIR", "./inbox");
        let poll_interval = dur_of("WATCH_POLL_INTERVAL", 5000, &mut errors);
        let process_existing = bool_of("WATCH_PROCESS_EXISTING", false, &mut errors);
        let move_after_process = bool_of("WATCH_MOVE_AFTER_PROCESS", true, &mut errors);
        let watch_dir = PathBuf::from(&watch_dir_raw);
        let processed_raw = text("WATCH_PROCESSED_DIR", "");
        let failed_raw = text("WATCH_FAILED_DIR", "");
        let processed_dir = if processed_raw.is_empty() {
            watch_dir.join("processed")
        } else {
            PathBuf::from(processed_raw)
        };
        let failed_dir = if failed_raw.is_empty() {
            watch_dir.join("failed")
        } else {
            PathBuf::from(failed_raw)
        };
        let stable_checks_raw = int_of("WATCH_STABLE_CHECKS", 2, &mut errors);
        let stable_checks = if stable_checks_raw < 1 {
            1
        } else {
            stable_checks_raw as u32
        };
        let max_file_bytes_raw = int_of("MAX_FILE_BYTES", 10_485_760, &mut errors);
        let max_file_bytes = if max_file_bytes_raw <= 0 {
            10_485_760
        } else {
            max_file_bytes_raw as u64
        };

        let status_enabled = bool_of("STATUS_ENABLED", true, &mut errors);
        let status_file_raw = text("STATUS_FILE", "./status/envios.jsonl");
        let status_file = PathBuf::from(&status_file_raw);

        let http_timeout = dur_of("HTTP_TIMEOUT", 30_000, &mut errors);
        let http_retries_raw = int_of("HTTP_RETRIES", 2, &mut errors);
        let http_retries = if http_retries_raw < 0 {
            0
        } else {
            http_retries_raw as u32
        };
        let retry_backoff = dur_of("HTTP_RETRY_BACKOFF", 2000, &mut errors);
        let log_level_raw = match raw("LOG_LEVEL") {
            Some(v) if v.is_empty() => "info".to_string(),
            Some(v) => v,
            None => "info".to_string(),
        };
        let log_level = parse_log_level(&log_level_raw);
        if log_level.is_none() {
            errors.push(format!(
                "LOG_LEVEL inválido: '{}' (use debug, info, warn o error)",
                log_level_raw
            ));
        }
        let log_level = log_level.unwrap_or(LogLevel::Info);

        if token_field.is_empty() {
            errors.push("OASIS_TOKEN_FIELD no puede estar vacío".to_string());
        }
        if file_field.is_empty() {
            errors.push("OASIS_FILE_FIELD no puede estar vacío".to_string());
        }
        if poll_interval.is_zero() {
            errors.push("WATCH_POLL_INTERVAL debe ser mayor a 0".to_string());
        }
        if http_timeout.is_zero() {
            errors.push("HTTP_TIMEOUT debe ser mayor a 0".to_string());
        }
        if watch_dir_raw.is_empty() {
            errors.push("WATCH_DIR no puede estar vacío".to_string());
        }
        if status_enabled && status_file_raw.is_empty() {
            errors.push("STATUS_FILE no puede estar vacío cuando STATUS_ENABLED=true".to_string());
        }
        if !is_mock {
            if context.is_empty() {
                errors.push("OASIS_CONTEXT es obligatorio cuando IS_MOCK=false".to_string());
            }
            if username.is_empty() {
                errors.push("OASIS_USERNAME es obligatorio cuando IS_MOCK=false".to_string());
            }
            if password.is_empty() {
                errors.push("OASIS_PASSWORD es obligatorio cuando IS_MOCK=false".to_string());
            }
            if company_code.is_empty() {
                errors.push("OASIS_COMPANY_CODE es obligatorio cuando IS_MOCK=false".to_string());
            }
            if publication_code.is_empty() {
                errors
                    .push("OASIS_PUBLICATION_CODE es obligatorio cuando IS_MOCK=false".to_string());
            }
        }

        if !errors.is_empty() {
            return Err(ConfigError { messages: errors });
        }

        Ok(Config {
            is_mock,
            mock_latency,
            mock_fail,
            token_url,
            context,
            username,
            password,
            client_id,
            client_secret,
            scope,
            grant_type,
            api_url,
            company_code,
            publication_code,
            request_format,
            token_field,
            file_field,
            trigger_watch,
            watch_dir,
            poll_interval,
            process_existing,
            move_after_process,
            processed_dir,
            failed_dir,
            stable_checks,
            max_file_bytes,
            status_enabled,
            status_file,
            http_timeout,
            http_retries,
            retry_backoff,
            log_level,
        })
    }
}

pub fn parse_bool(v: &str) -> Option<bool> {
    match v.trim().to_lowercase().as_str() {
        "true" | "1" | "yes" | "on" | "t" | "y" => Some(true),
        "false" | "0" | "no" | "off" | "f" | "n" => Some(false),
        _ => None,
    }
}

pub fn parse_duration(v: &str) -> Option<Duration> {
    let v = v.trim().to_lowercase();
    let (number, unit) = if let Some(n) = v.strip_suffix("ms") {
        (n, 0.001)
    } else if let Some(n) = v.strip_suffix('s') {
        (n, 1.0)
    } else if let Some(n) = v.strip_suffix('m') {
        (n, 60.0)
    } else if let Some(n) = v.strip_suffix('h') {
        (n, 3600.0)
    } else {
        return None;
    };
    let number = number.trim();
    if number.is_empty() {
        return None;
    }
    let value: f64 = number.parse().ok()?;
    if !value.is_finite() || value < 0.0 {
        return None;
    }
    Some(Duration::from_secs_f64(value * unit))
}

fn parse_log_level(v: &str) -> Option<LogLevel> {
    match v.trim().to_lowercase().as_str() {
        "debug" => Some(LogLevel::Debug),
        "info" => Some(LogLevel::Info),
        "warn" | "warning" => Some(LogLevel::Warn),
        "error" => Some(LogLevel::Error),
        _ => None,
    }
}

fn find_env_file() -> Option<PathBuf> {
    let cwd = Path::new(".env");
    if cwd.is_file() {
        return Some(cwd.to_path_buf());
    }
    let exe = std::env::current_exe().ok()?;
    let beside = exe.parent()?.join(".env");
    if beside.is_file() {
        return Some(beside);
    }
    None
}

pub fn parse_env_file(path: &Path) -> HashMap<String, String> {
    let mut map = HashMap::new();
    let Ok(content) = std::fs::read_to_string(path) else {
        return map;
    };
    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let line = line.strip_prefix("export ").unwrap_or(line);
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let key = key.trim();
        if key.is_empty() {
            continue;
        }
        let mut value = value.trim();
        if value.len() >= 2
            && ((value.starts_with('"') && value.ends_with('"'))
                || (value.starts_with('\'') && value.ends_with('\'')))
        {
            value = &value[1..value.len() - 1];
        }
        map.insert(key.to_string(), value.to_string());
    }
    map
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg(pairs: &[(&str, &str)]) -> Result<Config, ConfigError> {
        let env: HashMap<String, String> = pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        Config::from_sources(&env, &HashMap::new())
    }

    #[test]
    fn defaults_with_mock() {
        let c = cfg(&[("IS_MOCK", "true")]).unwrap();
        assert_eq!(c.watch_dir, PathBuf::from("./inbox"));
        assert_eq!(c.processed_dir, PathBuf::from("./inbox/processed"));
        assert_eq!(c.failed_dir, PathBuf::from("./inbox/failed"));
        assert_eq!(c.request_format, RequestFormat::Multipart);
        assert_eq!(c.http_timeout, Duration::from_secs(30));
        assert!(c.status_enabled);
        assert_eq!(c.status_file, PathBuf::from("./status/envios.jsonl"));
    }

    #[test]
    fn env_wins_over_file() {
        let env = HashMap::from([("IS_MOCK".to_string(), "true".to_string())]);
        let file = HashMap::from([
            ("IS_MOCK".to_string(), "false".to_string()),
            ("WATCH_DIR".to_string(), "/tmp/x".to_string()),
        ]);
        let c = Config::from_sources(&env, &file).unwrap();
        assert!(c.is_mock);
        assert_eq!(c.watch_dir, PathBuf::from("/tmp/x"));
    }

    #[test]
    fn invalid_values_reported_together() {
        let err = cfg(&[
            ("IS_MOCK", "true"),
            ("WATCH_POLL_INTERVAL", "cada rato"),
            ("OASIS_REQUEST_FORMAT", "form-data"),
        ])
        .unwrap_err();
        assert_eq!(err.messages.len(), 2);

        let err = cfg(&[("IS_MOCK", "quizas")]).unwrap_err();
        assert!(err.messages.iter().any(|m| m.contains("IS_MOCK")));
    }

    #[test]
    fn credentials_required_in_real_mode() {
        let err = cfg(&[]).unwrap_err();
        for key in [
            "OASIS_CONTEXT",
            "OASIS_USERNAME",
            "OASIS_PASSWORD",
            "OASIS_COMPANY_CODE",
            "OASIS_PUBLICATION_CODE",
        ] {
            assert!(err.messages.iter().any(|m| m.contains(key)), "{key}");
        }
    }

    #[test]
    fn booleans_and_durations() {
        assert_eq!(parse_bool("YES"), Some(true));
        assert_eq!(parse_bool("0"), Some(false));
        assert_eq!(parse_duration("300ms"), Some(Duration::from_millis(300)));
        assert_eq!(parse_duration("1.5s"), Some(Duration::from_millis(1500)));
        assert_eq!(parse_duration("2m"), Some(Duration::from_secs(120)));
        assert_eq!(parse_duration("1h"), Some(Duration::from_secs(3600)));
        assert_eq!(parse_duration("300MS"), Some(Duration::from_millis(300)));
        assert_eq!(parse_duration("2 m"), Some(Duration::from_secs(120)));
        let c = cfg(&[("IS_MOCK", "true"), ("LOG_LEVEL", "")]).unwrap();
        assert_eq!(c.log_level, LogLevel::Info);
    }

    #[test]
    fn env_parser_rules() {
        let dir = std::env::temp_dir().join("oasis_env_test");
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join(".env");
        std::fs::write(
            &p,
            "# c\n\nexport A=1\nB = 'dos'\nC=\"tres\"\nSINIGUAL\n= vacio\nD=x=y\n",
        )
        .unwrap();
        let m = parse_env_file(&p);
        assert_eq!(m.get("A").unwrap(), "1");
        assert_eq!(m.get("B").unwrap(), "dos");
        assert_eq!(m.get("C").unwrap(), "tres");
        assert_eq!(m.get("D").unwrap(), "x=y");
        assert!(!m.contains_key("SINIGUAL"));
        assert!(!m.contains_key(""));
    }
}
