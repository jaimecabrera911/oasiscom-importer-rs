use crate::config::{parse_bool, parse_duration};
use dialoguer::{Input, Password, Select};
use std::collections::HashMap;
use std::io::{self, BufRead, IsTerminal, Write};
use std::path::PathBuf;

#[derive(Clone, Copy)]
enum Kind {
    Bool,
    Int,
    Duration,
    Text,
    Choice(&'static [&'static str]),
}

struct VarDef {
    key: &'static str,
    default: &'static str,
    description: &'static str,
    group: &'static str,
    kind: Kind,
    secret: bool,
}

const fn var(
    key: &'static str,
    default: &'static str,
    description: &'static str,
    group: &'static str,
    kind: Kind,
    secret: bool,
) -> VarDef {
    VarDef {
        key,
        default,
        description,
        group,
        kind,
        secret,
    }
}

const FORMATS: &[&str] = &["multipart", "urlencoded"];
const LOG_LEVELS: &[&str] = &["debug", "info", "warn", "warning", "error"];

const VARS: &[VarDef] = &[
    var(
        "IS_MOCK",
        "false",
        "simula token y carga sin HTTP",
        "Simulación",
        Kind::Bool,
        false,
    ),
    var(
        "MOCK_LATENCY",
        "300ms",
        "espera artificial por llamada simulada",
        "Simulación",
        Kind::Duration,
        false,
    ),
    var(
        "MOCK_FAIL",
        "false",
        "fuerza que toda carga simulada falle",
        "Simulación",
        Kind::Bool,
        false,
    ),
    var(
        "OASIS_TOKEN_URL",
        "https://identity.oasiscom.com/connect/token",
        "endpoint del token",
        "Token",
        Kind::Text,
        false,
    ),
    var(
        "OASIS_CONTEXT",
        "",
        "conexión de OasisCom",
        "Token",
        Kind::Text,
        false,
    ),
    var(
        "OASIS_USERNAME",
        "",
        "usuario de OasisCom",
        "Token",
        Kind::Text,
        false,
    ),
    var(
        "OASIS_PASSWORD",
        "",
        "contraseña de OasisCom",
        "Token",
        Kind::Text,
        true,
    ),
    var(
        "OASIS_CLIENT_ID",
        "76db5426-2831-4105-9012-209119e53b5e",
        "client ID",
        "Token",
        Kind::Text,
        false,
    ),
    var(
        "OASIS_CLIENT_SECRET",
        "511536EF-F270-4058-80CA-1C89C192F69A",
        "client secret",
        "Token",
        Kind::Text,
        true,
    ),
    var(
        "OASIS_SCOPE",
        "OasisCo",
        "scope del token",
        "Token",
        Kind::Text,
        false,
    ),
    var(
        "OASIS_GRANT_TYPE",
        "password",
        "grant type del token",
        "Token",
        Kind::Text,
        false,
    ),
    var(
        "OASIS_API_URL",
        "https://app.oasiscom.com/api/externalservices",
        "URL base de carga",
        "API externa",
        Kind::Text,
        false,
    ),
    var(
        "OASIS_COMPANY_CODE",
        "",
        "código de empresa",
        "API externa",
        Kind::Text,
        false,
    ),
    var(
        "OASIS_PUBLICATION_CODE",
        "",
        "código de publicación",
        "API externa",
        Kind::Text,
        false,
    ),
    var(
        "OASIS_REQUEST_FORMAT",
        "multipart",
        "formato del cuerpo",
        "API externa",
        Kind::Choice(FORMATS),
        false,
    ),
    var(
        "OASIS_TOKEN_FIELD",
        "token",
        "campo del token",
        "API externa",
        Kind::Text,
        false,
    ),
    var(
        "OASIS_FILE_FIELD",
        "FileBase64",
        "campo del XML en Base64",
        "API externa",
        Kind::Text,
        false,
    ),
    var(
        "TRIGGER_WATCH",
        "true",
        "vigilancia continua o lote único",
        "Disparador y archivos",
        Kind::Bool,
        false,
    ),
    var(
        "WATCH_DIR",
        "./inbox",
        "carpeta observada",
        "Disparador y archivos",
        Kind::Text,
        false,
    ),
    var(
        "WATCH_POLL_INTERVAL",
        "5s",
        "intervalo de sondeo",
        "Disparador y archivos",
        Kind::Duration,
        false,
    ),
    var(
        "WATCH_PROCESS_EXISTING",
        "false",
        "procesa XML previos al arrancar",
        "Disparador y archivos",
        Kind::Bool,
        false,
    ),
    var(
        "WATCH_MOVE_AFTER_PROCESS",
        "true",
        "mueve el XML tras procesarlo",
        "Disparador y archivos",
        Kind::Bool,
        false,
    ),
    var(
        "WATCH_PROCESSED_DIR",
        "",
        "destino de exitosos (defecto WATCH_DIR/processed)",
        "Disparador y archivos",
        Kind::Text,
        false,
    ),
    var(
        "WATCH_FAILED_DIR",
        "",
        "destino de fallidos (defecto WATCH_DIR/failed)",
        "Disparador y archivos",
        Kind::Text,
        false,
    ),
    var(
        "WATCH_STABLE_CHECKS",
        "2",
        "sondeos estables requeridos",
        "Disparador y archivos",
        Kind::Int,
        false,
    ),
    var(
        "MAX_FILE_BYTES",
        "10485760",
        "tamaño máximo del XML",
        "Disparador y archivos",
        Kind::Int,
        false,
    ),
    var(
        "STATUS_ENABLED",
        "true",
        "registra cada intento",
        "Estado",
        Kind::Bool,
        false,
    ),
    var(
        "STATUS_FILE",
        "./status/envios.jsonl",
        "ruta del historial JSONL",
        "Estado",
        Kind::Text,
        false,
    ),
    var(
        "HTTP_TIMEOUT",
        "30s",
        "timeout por petición",
        "HTTP y logs",
        Kind::Duration,
        false,
    ),
    var(
        "HTTP_RETRIES",
        "2",
        "reintentos adicionales",
        "HTTP y logs",
        Kind::Int,
        false,
    ),
    var(
        "HTTP_RETRY_BACKOFF",
        "2s",
        "base de espera entre reintentos",
        "HTTP y logs",
        Kind::Duration,
        false,
    ),
    var(
        "LOG_LEVEL",
        "info",
        "nivel de logs",
        "HTTP y logs",
        Kind::Choice(LOG_LEVELS),
        false,
    ),
    var(
        "LOG_FILE",
        "./logs/oasis.log",
        "archivo de logs (vacío = sin archivo)",
        "HTTP y logs",
        Kind::Text,
        false,
    ),
];

/// Contenido con el que se crea el `.env` cuando no existe (mismo que `.env.example`).
const DEFAULT_ENV: &str = include_str!("../.env.example");

fn ensure_env_file(path: &std::path::Path) -> io::Result<bool> {
    if path.is_file() {
        return Ok(false);
    }
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)?;
        }
    }
    std::fs::write(path, DEFAULT_ENV)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
    }
    Ok(true)
}

pub fn run(args: &[String]) -> i32 {
    let path = crate::config::env_file_for_edit();

    let manages_env = matches!(
        args.first().map(|s| s.as_str()),
        None | Some("list") | Some("show") | Some("get") | Some("set") | Some("unset")
    );
    if manages_env {
        match ensure_env_file(&path) {
            Ok(true) => eprintln!("creado {} con los valores por defecto", path.display()),
            Ok(false) => {}
            Err(e) => {
                eprintln!("error creando .env: {e}");
                return 1;
            }
        }
    }

    let mut editor = match Editor::load(path) {
        Ok(e) => e,
        Err(e) => {
            eprintln!("error leyendo .env: {e}");
            return 1;
        }
    };

    match args.first().map(|s| s.as_str()) {
        None => {
            if io::stdin().is_terminal() && io::stdout().is_terminal() {
                interactive_tui(&mut editor)
            } else {
                interactive(&mut editor)
            }
        }
        Some("list") | Some("show") if args.len() == 1 => {
            print_list(&editor);
            0
        }
        Some("get") => {
            let Some(key) = args.get(1) else {
                return usage("falta la clave: oasis config get CLAVE");
            };
            let Some(var) = find_var(key) else {
                return usage(&format!("variable desconocida: {key}"));
            };
            let (value, source) = editor.effective(var);
            println!("{}={} ({source})", var.key, display(var, &value));
            0
        }
        Some("set") => {
            let Some(key) = args.get(1) else {
                return usage("falta la clave: oasis config set CLAVE VALOR");
            };
            let Some(var) = find_var(key) else {
                return usage(&format!("variable desconocida: {key}"));
            };
            let value = args[2..].join(" ");
            if let Err(e) = validate(var, &value) {
                return usage(&format!("valor inválido para {}: {e}", var.key));
            }
            editor.set(var.key, &value);
            if let Err(e) = editor.save() {
                eprintln!("error guardando .env: {e}");
                return 1;
            }
            println!("{}={}", var.key, display(var, &value));
            warn_env_shadow(var.key);
            0
        }
        Some("unset") => {
            let Some(key) = args.get(1) else {
                return usage("falta la clave: oasis config unset CLAVE");
            };
            let Some(var) = find_var(key) else {
                return usage(&format!("variable desconocida: {key}"));
            };
            editor.unset(var.key);
            if let Err(e) = editor.save() {
                eprintln!("error guardando .env: {e}");
                return 1;
            }
            println!(
                "{} eliminada de .env (se usará el valor por defecto)",
                var.key
            );
            warn_env_shadow(var.key);
            0
        }
        Some("help") | Some("-h") | Some("--help") => {
            print_usage();
            0
        }
        Some(other) => usage(&format!("subcomando desconocido: {other}")),
    }
}

const BOOLS: &[&str] = &["true", "false"];

enum EditOutcome {
    Set(String),
    Default,
    Back,
    Abort,
}

fn interactive_tui(editor: &mut Editor) -> i32 {
    loop {
        let mut labels = Vec::with_capacity(VARS.len() + 1);
        for var in VARS {
            let (value, source) = editor.effective(var);
            labels.push(format!(
                "{:<24} {:<36} [{}] {}",
                var.key,
                display(var, &value),
                source,
                var.group
            ));
        }
        labels.push("Salir".to_string());

        let prompt = format!("Configuración: {}  (↑/↓ y Enter)", editor.path.display());
        let selection = match Select::new()
            .with_prompt(prompt)
            .items(&labels)
            .default(0)
            .interact()
        {
            Ok(index) => index,
            Err(_) => {
                println!();
                return 0;
            }
        };
        if selection == VARS.len() {
            return 0;
        }
        let var = &VARS[selection];
        let (current, _) = editor.effective(var);
        let outcome = match var.kind {
            Kind::Bool => {
                let index = if parse_bool(&current).unwrap_or(false) {
                    0
                } else {
                    1
                };
                choose(var, BOOLS, index, &current)
            }
            Kind::Choice(options) => {
                let index = options
                    .iter()
                    .position(|o| o.eq_ignore_ascii_case(&current))
                    .unwrap_or(0);
                choose(var, options, index, &current)
            }
            _ => edit_free(var, &current),
        };

        match outcome {
            EditOutcome::Abort => {
                println!();
                return 0;
            }
            EditOutcome::Back => continue,
            EditOutcome::Default => editor.unset(var.key),
            EditOutcome::Set(value) => {
                if let Err(e) = validate(var, &value) {
                    eprintln!("\nValor inválido: {e}");
                    continue;
                }
                editor.set(var.key, &value);
            }
        }
        if let Err(e) = editor.save() {
            eprintln!("error guardando .env: {e}");
            return 1;
        }
        warn_env_shadow(var.key);
    }
}

fn choose(
    var: &VarDef,
    options: &[&'static str],
    current_index: usize,
    current: &str,
) -> EditOutcome {
    let mut items: Vec<String> = options.iter().map(|o| o.to_string()).collect();
    items.push("Volver sin cambios".to_string());
    let prompt = format!("{} = {}  (↑/↓ y Enter)", var.key, display(var, current));
    match Select::new()
        .with_prompt(prompt)
        .items(&items)
        .default(current_index)
        .interact()
    {
        Ok(index) if index < options.len() => EditOutcome::Set(options[index].to_string()),
        Ok(_) => EditOutcome::Back,
        Err(_) => EditOutcome::Abort,
    }
}

fn edit_free(var: &VarDef, current: &str) -> EditOutcome {
    if var.secret {
        let prompt = format!("{} (nuevo valor; Enter cancela)", var.key);
        match Password::new()
            .with_prompt(prompt)
            .allow_empty_password(true)
            .interact()
        {
            Ok(value) if value.trim().is_empty() => EditOutcome::Back,
            Ok(value) => EditOutcome::Set(value),
            Err(_) => EditOutcome::Abort,
        }
    } else {
        let prompt = format!(
            "{} — {} (Enter deja el valor actual; - usa el defecto)",
            var.key, var.description
        );
        match Input::<String>::new()
            .with_prompt(prompt)
            .with_initial_text(current.to_string())
            .allow_empty(true)
            .interact_text()
        {
            Ok(value) => {
                let value = value.trim();
                if value.is_empty() || value == current {
                    EditOutcome::Back
                } else if value == "-" {
                    EditOutcome::Default
                } else {
                    EditOutcome::Set(value.to_string())
                }
            }
            Err(_) => EditOutcome::Abort,
        }
    }
}

fn interactive(editor: &mut Editor) -> i32 {
    let stdin = io::stdin();
    loop {
        print_menu(editor);
        println!("  0) Salir");
        print!("\nSelecciona una variable: ");
        let _ = io::stdout().flush();
        let Some(line) = read_line(&stdin) else {
            println!();
            return 0;
        };
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let index: usize = match line.parse() {
            Ok(n) => n,
            Err(_) => {
                eprintln!("Opción inválida.");
                continue;
            }
        };
        if index == 0 {
            println!("Configuración guardada en {}.", editor.path.display());
            return 0;
        }
        let Some(var) = VARS.get(index - 1) else {
            eprintln!("Opción fuera de rango.");
            continue;
        };

        let action = match prompt_value(&stdin, editor, var) {
            Some(action) => action,
            None => return 0,
        };
        match action {
            Action::Cancel => continue,
            Action::Default => editor.unset(var.key),
            Action::Set(value) => {
                if let Err(e) = validate(var, &value) {
                    eprintln!("Valor inválido: {e}");
                    continue;
                }
                editor.set(var.key, &value);
            }
        }
        if let Err(e) = editor.save() {
            eprintln!("error guardando .env: {e}");
            return 1;
        }
        println!("Guardado en {}.", editor.path.display());
        warn_env_shadow(var.key);
    }
}

enum Action {
    Cancel,
    Default,
    Set(String),
}

fn prompt_value(stdin: &io::Stdin, editor: &Editor, var: &VarDef) -> Option<Action> {
    let (current, source) = editor.effective(var);
    match var.kind {
        Kind::Bool => loop {
            println!("\n{} = {} ({source})", var.key, display(var, &current));
            println!("  1) true");
            println!("  2) false");
            print!("Selecciona [1/2] (Enter cancela, - valor por defecto): ");
            let _ = io::stdout().flush();
            let line = read_line(stdin)?;
            match line.trim() {
                "" => return Some(Action::Cancel),
                "-" => return Some(Action::Default),
                "1" => return Some(Action::Set("true".to_string())),
                "2" => return Some(Action::Set("false".to_string())),
                _ => eprintln!("Opción inválida; escribe 1 o 2."),
            }
        },
        Kind::Choice(options) => loop {
            println!("\n{} = {} ({source})", var.key, display(var, &current));
            for (index, option) in options.iter().enumerate() {
                let marker = if option.eq_ignore_ascii_case(&current) {
                    " (actual)"
                } else {
                    ""
                };
                println!("  {}) {option}{marker}", index + 1);
            }
            print!(
                "Selecciona [1-{}] (Enter cancela, - valor por defecto): ",
                options.len()
            );
            let _ = io::stdout().flush();
            let line = read_line(stdin)?;
            let line = line.trim();
            match line {
                "" => return Some(Action::Cancel),
                "-" => return Some(Action::Default),
                _ => match line.parse::<usize>() {
                    Ok(n) if n >= 1 && n <= options.len() => {
                        return Some(Action::Set(options[n - 1].to_string()))
                    }
                    _ => eprintln!("Opción inválida."),
                },
            }
        },
        _ => {
            println!(
                "\n{} = {} ({source}) — {}",
                var.key,
                display(var, &current),
                var.description
            );
            print!("Nuevo valor (Enter cancela, - valor por defecto): ");
            let _ = io::stdout().flush();
            let line = read_line(stdin)?;
            let value = line.trim();
            if value.is_empty() {
                Some(Action::Cancel)
            } else if value == "-" {
                Some(Action::Default)
            } else {
                Some(Action::Set(value.to_string()))
            }
        }
    }
}

fn read_line(stdin: &io::Stdin) -> Option<String> {
    let mut line = String::new();
    match stdin.lock().read_line(&mut line) {
        Ok(0) => None,
        Ok(_) => Some(line),
        Err(_) => None,
    }
}

fn print_usage() {
    println!("Uso:");
    println!("  oasis config                 Edita las variables de forma interactiva");
    println!("  oasis config list            Muestra las variables efectivas");
    println!("  oasis config get CLAVE       Muestra una variable");
    println!("  oasis config set CLAVE VALOR Guarda una variable en .env");
    println!("  oasis config unset CLAVE     Elimina una variable de .env");
}

fn usage(message: &str) -> i32 {
    eprintln!("error: {message}\n");
    print_usage();
    2
}

fn find_var(key: &str) -> Option<&'static VarDef> {
    VARS.iter().find(|v| v.key == key)
}

fn validate(var: &VarDef, value: &str) -> Result<(), String> {
    if value.contains(['\n', '\r']) {
        return Err("no se admiten saltos de línea".to_string());
    }
    if value.is_empty() {
        return Ok(());
    }
    match var.kind {
        Kind::Bool => parse_bool(value)
            .map(|_| ())
            .ok_or_else(|| "use true/false, 1/0, yes/no, on/off".to_string()),
        Kind::Int => value
            .parse::<i64>()
            .map(|_| ())
            .map_err(|_| "se esperaba un número entero".to_string()),
        Kind::Duration => parse_duration(value)
            .map(|_| ())
            .ok_or_else(|| "use por ejemplo 300ms, 5s, 2m o 1h".to_string()),
        Kind::Text => Ok(()),
        Kind::Choice(options) => {
            if options.iter().any(|o| o.eq_ignore_ascii_case(value)) {
                Ok(())
            } else {
                Err(format!("valores permitidos: {}", options.join(", ")))
            }
        }
    }
}

fn display(var: &VarDef, value: &str) -> String {
    if value.is_empty() {
        return "(vacío)".to_string();
    }
    if var.secret {
        return "***".to_string();
    }
    value.to_string()
}

fn print_list(editor: &Editor) {
    println!("Configuración: {}", editor.path.display());
    let mut group = "";
    for var in VARS {
        if var.group != group {
            group = var.group;
            println!("\n[{group}]");
        }
        let (value, source) = editor.effective(var);
        println!(
            "  {:<24} = {:<40} ({source}) — {}",
            var.key,
            display(var, &value),
            var.description
        );
    }
}

fn print_menu(editor: &Editor) {
    println!("\nConfiguración: {}", editor.path.display());
    let mut group = "";
    for (index, var) in VARS.iter().enumerate() {
        if var.group != group {
            group = var.group;
            println!("\n[{group}]");
        }
        let (value, source) = editor.effective(var);
        println!(
            "  {:>2}) {:<24} = {} ({source})",
            index + 1,
            var.key,
            display(var, &value)
        );
    }
}

fn warn_env_shadow(key: &str) {
    if std::env::var_os(key).is_some() {
        eprintln!("nota: {key} también está definida en el entorno y tiene prioridad sobre .env");
    }
}

struct Editor {
    path: PathBuf,
    lines: Vec<String>,
    values: HashMap<String, String>,
}

impl Editor {
    fn load(path: PathBuf) -> io::Result<Self> {
        let content = match std::fs::read_to_string(&path) {
            Ok(c) => c,
            Err(e) if e.kind() == io::ErrorKind::NotFound => String::new(),
            Err(e) => return Err(e),
        };
        let lines: Vec<String> = content.lines().map(|l| l.to_string()).collect();
        let mut values = HashMap::new();
        for line in &lines {
            if let Some((key, value)) = parse_line(line) {
                values.insert(key, value);
            }
        }
        Ok(Editor {
            path,
            lines,
            values,
        })
    }

    fn effective(&self, var: &VarDef) -> (String, &'static str) {
        if let Some(value) = std::env::var_os(var.key) {
            return (value.to_string_lossy().to_string(), "entorno");
        }
        if let Some(value) = self.values.get(var.key) {
            return (value.clone(), ".env");
        }
        (var.default.to_string(), "defecto")
    }

    fn set(&mut self, key: &str, value: &str) {
        let mut found = false;
        for line in &mut self.lines {
            if parse_line(line).map(|(k, _)| k).as_deref() == Some(key) {
                *line = format!("{key}={value}");
                found = true;
            }
        }
        if !found {
            self.lines.push(format!("{key}={value}"));
        }
        self.values.insert(key.to_string(), value.to_string());
    }

    fn unset(&mut self, key: &str) {
        self.lines
            .retain(|line| parse_line(line).map(|(k, _)| k).as_deref() != Some(key));
        self.values.remove(key);
    }

    fn save(&self) -> io::Result<()> {
        if let Some(parent) = self.path.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent)?;
            }
        }
        let mut content = self.lines.join("\n");
        content.push('\n');
        let tmp = self.path.with_file_name(format!(
            "{}.tmp",
            self.path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| "env".to_string())
        ));
        std::fs::write(&tmp, content)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o600))?;
        }
        std::fs::rename(&tmp, &self.path)?;
        Ok(())
    }
}

fn parse_line(line: &str) -> Option<(String, String)> {
    let trimmed = line.trim();
    if trimmed.is_empty() || trimmed.starts_with('#') {
        return None;
    }
    let trimmed = trimmed.strip_prefix("export ").unwrap_or(trimmed);
    let (key, value) = trimmed.split_once('=')?;
    let key = key.trim();
    if key.is_empty() {
        return None;
    }
    let mut value = value.trim();
    if value.len() >= 2
        && ((value.starts_with('"') && value.ends_with('"'))
            || (value.starts_with('\'') && value.ends_with('\'')))
    {
        value = &value[1..value.len() - 1];
    }
    Some((key.to_string(), value.to_string()))
}
