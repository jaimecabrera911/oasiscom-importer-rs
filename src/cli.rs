use crate::config::Config;
use crate::status::{self, Event};
use chrono::{Local, NaiveDate};

struct Options {
    json: bool,
    estado: String,
    archivo: String,
    pedido: String,
    desde: Option<NaiveDate>,
    hasta: Option<NaiveDate>,
    limit: Option<usize>,
    limit_explicit: bool,
}

pub fn status_command(cfg: &Config, args: &[String]) -> i32 {
    let mut opts = Options {
        json: false,
        estado: String::new(),
        archivo: String::new(),
        pedido: String::new(),
        desde: None,
        hasta: None,
        limit: None,
        limit_explicit: false,
    };

    let mut i = 0;
    while i < args.len() {
        let flag = args[i].as_str();
        let value = |i: usize| -> Option<&str> { args.get(i + 1).map(|s| s.as_str()) };
        match flag {
            "--json" => {
                opts.json = true;
                i += 1;
            }
            "--estado" => {
                let Some(v) = value(i) else {
                    return usage_error("falta el valor de --estado");
                };
                let v = v.trim().to_uppercase();
                if v != "ENVIADO" && v != "FALLIDO" {
                    return usage_error(&format!(
                        "valor inválido para --estado: '{}' (use ENVIADO o FALLIDO)",
                        v
                    ));
                }
                opts.estado = v;
                i += 2;
            }
            "--archivo" => {
                let Some(v) = value(i) else {
                    return usage_error("falta el valor de --archivo");
                };
                opts.archivo = v.to_lowercase();
                i += 2;
            }
            "--pedido" => {
                let Some(v) = value(i) else {
                    return usage_error("falta el valor de --pedido");
                };
                opts.pedido = v.to_lowercase();
                i += 2;
            }
            "--desde" | "--hasta" => {
                let Some(v) = value(i) else {
                    return usage_error(&format!("falta el valor de {flag}"));
                };
                let date = match NaiveDate::parse_from_str(v, "%Y-%m-%d") {
                    Ok(d) => d,
                    Err(_) => {
                        return usage_error(&format!(
                            "fecha inválida para {flag}: '{v}' (use AAAA-MM-DD)"
                        ))
                    }
                };
                if flag == "--desde" {
                    opts.desde = Some(date);
                } else {
                    opts.hasta = Some(date);
                }
                i += 2;
            }
            "--limit" => {
                let Some(v) = value(i) else {
                    return usage_error("falta el valor de --limit");
                };
                let n: i64 = match v.parse() {
                    Ok(n) => n,
                    Err(_) => return usage_error(&format!("valor inválido para --limit: '{v}'")),
                };
                if n < 0 {
                    return usage_error("--limit no puede ser negativo");
                }
                opts.limit = Some(n as usize);
                opts.limit_explicit = true;
                i += 2;
            }
            other => {
                return usage_error(&format!("flag desconocido: '{other}'"));
            }
        }
    }

    if let (Some(desde), Some(hasta)) = (opts.desde, opts.hasta) {
        if desde > hasta {
            return usage_error("--desde no puede ser posterior a --hasta");
        }
    }

    let read = match status::read_events(&cfg.status_file) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("error leyendo historial {}: {e}", cfg.status_file.display());
            return 1;
        }
    };

    let summary = status::summarize(&read.events);
    let filtered = filter_events(&read.events, &opts);
    let sorted = status::sorted_desc(&filtered);
    let total_filtered = sorted.len();

    let limit = if opts.json && !opts.limit_explicit {
        None
    } else {
        Some(opts.limit.unwrap_or(20))
    };
    let shown: Vec<Event> = match limit {
        Some(0) | None => sorted,
        Some(n) => sorted.into_iter().take(n).collect(),
    };

    let has_content_filter = !opts.estado.is_empty()
        || !opts.archivo.is_empty()
        || !opts.pedido.is_empty()
        || opts.desde.is_some()
        || opts.hasta.is_some();
    let diagnostics: Box<dyn Fn(&str)> = if opts.json {
        Box::new(|line: &str| eprintln!("{line}"))
    } else {
        Box::new(|line: &str| println!("{line}"))
    };

    diagnostics(&format!("Registro: {}", cfg.status_file.display()));
    diagnostics(&format!(
        "Eventos: {} | ENVIADO: {} | FALLIDO: {} | archivos únicos: {} | con más de un intento: {}",
        summary.total,
        summary.enviados,
        summary.fallidos,
        summary.archivos_unicos,
        summary.con_mas_de_un_intento
    ));
    if let Some(last) = &summary.ultimo {
        let fecha = format_fecha(&last.fecha);
        diagnostics(&format!(
            "Último: {fecha} | {} | {}",
            last.archivo, last.estado
        ));
    }
    if has_content_filter {
        diagnostics(&format!("Coincidencias: {total_filtered}"));
    }
    if read.invalid > 0 {
        diagnostics(&format!(
            "advertencia: {} línea(s) inválida(s) en el historial",
            read.invalid
        ));
    }
    if !cfg.status_enabled {
        diagnostics("advertencia: STATUS_ENABLED=false; no se registrarán nuevos envíos");
    }

    if total_filtered == 0 {
        diagnostics("Sin registros que coincidan con los filtros.");
        return 0;
    }

    if opts.json {
        for event in &shown {
            match serde_json::to_string(event) {
                Ok(line) => println!("{line}"),
                Err(e) => {
                    eprintln!("error serializando evento: {e}");
                    return 1;
                }
            }
        }
    } else {
        print_table(&shown);
    }

    if shown.len() < total_filtered {
        diagnostics(&format!(
            "Mostrando {} de {} eventos (usa --limit 0 para ver todos).",
            shown.len(),
            total_filtered
        ));
    }
    0
}

fn usage_error(message: &str) -> i32 {
    eprintln!("error: {message}");
    2
}

fn filter_events(events: &[Event], opts: &Options) -> Vec<Event> {
    events
        .iter()
        .filter(|event| {
            if !opts.estado.is_empty() && event.estado.to_uppercase() != opts.estado {
                return false;
            }
            if !opts.archivo.is_empty() && !event.archivo.to_lowercase().contains(&opts.archivo) {
                return false;
            }
            if !opts.pedido.is_empty()
                && !event
                    .documentos
                    .iter()
                    .any(|d| d.pedido.to_lowercase().contains(&opts.pedido))
            {
                return false;
            }
            let date = status::fecha_key(event).map(|f| f.with_timezone(&Local).date_naive());
            if let Some(desde) = opts.desde {
                if date.map(|d| d < desde).unwrap_or(true) {
                    return false;
                }
            }
            if let Some(hasta) = opts.hasta {
                if date.map(|d| d > hasta).unwrap_or(true) {
                    return false;
                }
            }
            true
        })
        .cloned()
        .collect()
}

fn format_fecha(fecha: &str) -> String {
    match chrono::DateTime::parse_from_rfc3339(fecha) {
        Ok(f) => f
            .with_timezone(&Local)
            .format("%Y-%m-%d %H:%M:%S")
            .to_string(),
        Err(_) => "-".to_string(),
    }
}

fn collapse(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn detail(event: &Event) -> String {
    let raw = if !event.error.is_empty() {
        &event.error
    } else {
        &event.mensaje
    };
    let collapsed = collapse(raw);
    if collapsed.is_empty() {
        return "-".to_string();
    }
    crate::processor::truncate(&collapsed, 80)
}

fn documento_label(event: &Event) -> String {
    let labels: Vec<String> = event
        .documentos
        .iter()
        .filter(|d| !d.documento.is_empty() || !d.numero.is_empty())
        .map(|d| format!("{}/{}", d.documento, d.numero))
        .collect();
    if labels.is_empty() {
        "-".to_string()
    } else {
        labels.join(",")
    }
}

fn pedido_label(event: &Event) -> String {
    let labels: Vec<&str> = event
        .documentos
        .iter()
        .map(|d| d.pedido.as_str())
        .filter(|p| !p.is_empty())
        .collect();
    if labels.is_empty() {
        "-".to_string()
    } else {
        labels.join(",")
    }
}

fn print_table(events: &[Event]) {
    let headers = [
        "FECHA",
        "ESTADO",
        "ARCHIVO",
        "DOCUMENTO",
        "PEDIDO",
        "HTTP",
        "DETALLE",
    ];
    let rows: Vec<[String; 7]> = events
        .iter()
        .map(|e| {
            [
                format_fecha(&e.fecha),
                e.estado.clone(),
                e.archivo.clone(),
                documento_label(e),
                pedido_label(e),
                if e.http == 0 {
                    "-".to_string()
                } else {
                    e.http.to_string()
                },
                detail(e),
            ]
        })
        .collect();

    let mut widths = [0usize; 7];
    for (idx, header) in headers.iter().enumerate() {
        widths[idx] = header.chars().count();
    }
    for row in &rows {
        for (idx, cell) in row.iter().enumerate() {
            widths[idx] = widths[idx].max(cell.chars().count());
        }
    }

    println!();
    print_row(&headers.map(|h| h.to_string()), &widths);
    for row in &rows {
        print_row(row, &widths);
    }
}

fn print_row(row: &[String; 7], widths: &[usize; 7]) {
    let mut line = String::new();
    for (idx, cell) in row.iter().enumerate() {
        if idx == row.len() - 1 {
            line.push_str(cell);
        } else {
            line.push_str(cell);
            let pad = widths[idx].saturating_sub(cell.chars().count()) + 2;
            line.push_str(&" ".repeat(pad));
        }
    }
    println!("{line}");
}

pub fn help_text() -> String {
    format!(
        r#"OasisCom XML Importer {version}

Uso:
  oasis [comando] [opciones]

Comandos:
  (sin comando)          Vigila o procesa un lote según TRIGGER_WATCH
  watch                  Fuerza la vigilancia de WATCH_DIR
  send <archivo...>      Envía los archivos indicados (alias: run)
  token                  Genera e imprime un token en stdout
  config                 Edita las variables de .env (interactivo)
  status [filtros]       Consulta el historial de envíos
  version, -v, --version Muestra la versión
  help, -h, --help       Muestra esta ayuda

Filtros de status:
  --json             Salida JSON Lines (diagnósticos por stderr)
  --estado VALOR     ENVIADO o FALLIDO
  --archivo TEXTO    Subcadena del nombre de archivo
  --pedido TEXTO     Subcadena de PURCHASEORDER
  --desde AAAA-MM-DD Incluye desde esa fecha
  --hasta AAAA-MM-DD Incluye hasta esa fecha
  --limit N          Filas a mostrar (0 = todas, defecto 20)

Configuración:
  config             Menú interactivo para editar .env
  config list        Muestra las variables efectivas
  config get CLAVE   Muestra una variable
  config set CLAVE VALOR
  config unset CLAVE
"#,
        version = crate::VERSION
    )
}
