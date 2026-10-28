mod cli;
mod client;
mod config;
mod config_cmd;
mod logger;
mod processor;
mod status;
mod watcher;
mod xml;

pub const VERSION: &str = "0.1.0";

use client::Client;
use config::Config;
use logger::Logger;
use processor::Processor;
use status::StatusStore;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use watcher::Watcher;

fn main() {
    std::process::exit(run());
}

fn run() -> i32 {
    let args: Vec<String> = std::env::args().skip(1).collect();

    // `config` se ejecuta antes de validar la configuración para poder corregirla.
    if args.first().map(|s| s.as_str()) == Some("config") {
        return config_cmd::run(&args[1..]);
    }

    let cfg = match Config::load() {
        Ok(cfg) => cfg,
        Err(e) => {
            eprintln!("error de configuración: {e}");
            return 2;
        }
    };
    let logger = Logger::new(cfg.log_level, &cfg.log_file);

    let stop = Arc::new(AtomicBool::new(false));
    let stop_flag = Arc::clone(&stop);
    if let Err(e) = ctrlc::set_handler(move || stop_flag.store(true, Ordering::SeqCst)) {
        logger.warn(
            "no se pudo instalar el manejador de señales",
            &[("error", e.to_string())],
        );
    }

    match args.first().map(|s| s.as_str()) {
        None => {
            if cfg.trigger_watch {
                run_watch(&cfg, &logger, &stop)
            } else {
                run_batch(&cfg, &logger, &stop)
            }
        }
        Some("watch") => run_watch(&cfg, &logger, &stop),
        Some("send") | Some("run") => {
            let files = &args[1..];
            if files.is_empty() {
                eprintln!("error: send requiere al menos un archivo");
                return 2;
            }
            run_send(&cfg, &logger, &stop, files)
        }
        Some("token") => run_token(&cfg, &logger),
        Some("status") => cli::status_command(&cfg, &args[1..]),
        Some("version") | Some("-v") | Some("--version") => {
            println!("oasis {VERSION}");
            0
        }
        Some("help") | Some("-h") | Some("--help") => {
            print!("{}", cli::help_text());
            0
        }
        Some(other) => {
            eprintln!("comando desconocido: {other}\n");
            eprint!("{}", cli::help_text());
            2
        }
    }
}

fn build<'a>(cfg: &'a Config, logger: &'a Logger) -> Result<(StatusStore, Client<'a>), String> {
    let client = Client::new(cfg, logger)?;
    let store = StatusStore::new(cfg.status_file.clone(), cfg.status_enabled);
    Ok((store, client))
}

fn run_watch(cfg: &Config, logger: &Logger, stop: &AtomicBool) -> i32 {
    let (store, client) = match build(cfg, logger) {
        Ok(v) => v,
        Err(e) => {
            logger.error("no se pudo iniciar", &[("error", e)]);
            return 1;
        }
    };
    let processor = Processor::new(cfg, &client, &store, logger);
    let watcher = Watcher::new(cfg, &processor, logger);
    watcher.run(stop)
}

fn run_batch(cfg: &Config, logger: &Logger, stop: &AtomicBool) -> i32 {
    let (store, client) = match build(cfg, logger) {
        Ok(v) => v,
        Err(e) => {
            logger.error("no se pudo iniciar", &[("error", e)]);
            return 1;
        }
    };
    let processor = Processor::new(cfg, &client, &store, logger);

    let entries = match std::fs::read_dir(&cfg.watch_dir) {
        Ok(e) => e,
        Err(e) => {
            logger.error(
                "no se pudo leer la carpeta",
                &[
                    ("carpeta", cfg.watch_dir.display().to_string()),
                    ("error", e.to_string()),
                ],
            );
            return 1;
        }
    };
    let mut files: Vec<PathBuf> = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        let is_xml = path
            .extension()
            .map(|e| e.to_string_lossy().to_lowercase() == "xml")
            .unwrap_or(false);
        if is_xml && entry.file_type().map(|t| t.is_file()).unwrap_or(false) {
            files.push(path);
        }
    }
    files.sort();
    if files.is_empty() {
        logger.warn(
            "no hay archivos XML para procesar",
            &[("carpeta", cfg.watch_dir.display().to_string())],
        );
        return 0;
    }

    let mut failed = false;
    for file in files {
        if stop.load(Ordering::SeqCst) {
            return 1;
        }
        if !processor.process_file(&file) {
            failed = true;
        }
    }
    if failed || stop.load(Ordering::SeqCst) {
        1
    } else {
        0
    }
}

fn run_send(cfg: &Config, logger: &Logger, stop: &AtomicBool, files: &[String]) -> i32 {
    let (store, client) = match build(cfg, logger) {
        Ok(v) => v,
        Err(e) => {
            logger.error("no se pudo iniciar", &[("error", e)]);
            return 1;
        }
    };
    let processor = Processor::new(cfg, &client, &store, logger);
    let mut failed = false;
    for file in files {
        if stop.load(Ordering::SeqCst) {
            return 1;
        }
        if !processor.process_file(Path::new(file)) {
            failed = true;
        }
    }
    if failed || stop.load(Ordering::SeqCst) {
        1
    } else {
        0
    }
}

fn run_token(cfg: &Config, logger: &Logger) -> i32 {
    let client = match Client::new(cfg, logger) {
        Ok(c) => c,
        Err(e) => {
            logger.error("no se pudo iniciar", &[("error", e)]);
            return 1;
        }
    };
    match client.token() {
        Ok(token) => {
            println!("{token}");
            0
        }
        Err(e) => {
            eprintln!("error generando token: {}", e.message);
            1
        }
    }
}
