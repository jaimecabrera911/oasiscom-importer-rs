use crate::client::Client;
use crate::config::Config;
use crate::logger::Logger;
use crate::status::{Event, StatusStore};
use crate::xml;
use chrono::Local;
use sha2::{Digest, Sha256};
use std::fs::File;
use std::io::{BufReader, Read};
use std::path::Path;
use std::time::Instant;

const MESSAGE_LIMIT: usize = 2000;

pub struct Processor<'a> {
    cfg: &'a Config,
    client: &'a Client<'a>,
    store: &'a StatusStore,
    logger: &'a Logger,
}

impl<'a> Processor<'a> {
    pub fn new(
        cfg: &'a Config,
        client: &'a Client<'a>,
        store: &'a StatusStore,
        logger: &'a Logger,
    ) -> Self {
        Processor {
            cfg,
            client,
            store,
            logger,
        }
    }

    /// Devuelve true si el envío fue exitoso.
    pub fn process_file(&self, path: &Path) -> bool {
        let start = Instant::now();
        let archivo = path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        let mut base = Event {
            archivo,
            empresa: self.cfg.company_code.clone(),
            mock: self.cfg.is_mock,
            ..Default::default()
        };

        let metadata = match std::fs::metadata(path) {
            Ok(m) => m,
            Err(e) => {
                base.error = format!("error de stat: {e}");
                return self.finish(base, start);
            }
        };
        let size = metadata.len();

        let hash = match sha256_file(path) {
            Ok(h) => h,
            Err(e) => {
                base.error = format!("error calculando hash: {e}");
                return self.finish(base, start);
            }
        };
        base.hash = hash;

        if size > self.cfg.max_file_bytes {
            base.error = format!(
                "archivo demasiado grande: {size} bytes (límite {})",
                self.cfg.max_file_bytes
            );
            return self.finish_with_move(path, base, start, false);
        }
        if size == 0 {
            base.error = "archivo vacío".to_string();
            return self.finish_with_move(path, base, start, false);
        }

        let content = match std::fs::read(path) {
            Ok(c) => c,
            Err(e) => {
                base.error = format!("error de lectura: {e}");
                return self.finish(base, start);
            }
        };

        let (xml_company, documents) = xml::extract(&content);
        if !xml_company.is_empty() {
            base.empresa = xml_company;
        }
        base.documentos = documents;

        self.logger.info(
            "procesando archivo",
            &[
                ("archivo", base.archivo.clone()),
                ("bytes", size.to_string()),
                ("mock", self.cfg.is_mock.to_string()),
                ("operaciones", base.documentos.len().to_string()),
            ],
        );

        match self.client.upload(&base.archivo, &content, &base.empresa) {
            Ok(result) => {
                base.estado = "ENVIADO".to_string();
                base.http = result.http;
                base.mensaje = truncate(&result.body, MESSAGE_LIMIT);
                self.finish_with_move(path, base, start, true)
            }
            Err(err) => {
                base.estado = "FALLIDO".to_string();
                base.http = err.http;
                base.mensaje = truncate(&err.body, MESSAGE_LIMIT);
                base.error = err.message;
                self.finish_with_move(path, base, start, false)
            }
        }
    }

    fn finish(&self, mut event: Event, start: Instant) -> bool {
        if event.estado.is_empty() {
            event.estado = "FALLIDO".to_string();
        }
        event.fecha = Local::now().to_rfc3339_opts(chrono::SecondsFormat::Micros, false);
        event.duracion_ms = start.elapsed().as_millis() as u64;
        if event.estado == "FALLIDO" {
            self.logger.error(
                "archivo fallido",
                &[
                    ("archivo", event.archivo.clone()),
                    ("http", err_http_text(event.http)),
                    ("error", event.error.clone()),
                ],
            );
        } else {
            self.logger.info(
                "archivo enviado",
                &[
                    ("archivo", event.archivo.clone()),
                    ("http", event.http.to_string()),
                    ("duracion_ms", event.duracion_ms.to_string()),
                ],
            );
        }
        if let Err(e) = self.store.append(&event) {
            self.logger.error(
                "no se pudo registrar el evento",
                &[("error", e.to_string())],
            );
        }
        event.estado == "ENVIADO"
    }

    fn finish_with_move(&self, path: &Path, event: Event, start: Instant, success: bool) -> bool {
        let executed = self.finish(event, start);
        self.maybe_move(path, success);
        executed
    }

    fn maybe_move(&self, path: &Path, success: bool) {
        if !self.cfg.move_after_process {
            return;
        }
        let watch_dir = match std::fs::canonicalize(&self.cfg.watch_dir) {
            Ok(p) => p,
            Err(_) => return,
        };
        let source = match std::fs::canonicalize(path) {
            Ok(p) => p,
            Err(_) => return,
        };
        if !source.starts_with(&watch_dir) || source == watch_dir {
            return;
        }
        let dest_dir = if success {
            &self.cfg.processed_dir
        } else {
            &self.cfg.failed_dir
        };
        if let Err(e) = std::fs::create_dir_all(dest_dir) {
            self.logger.warn(
                "no se pudo crear carpeta destino",
                &[
                    ("carpeta", dest_dir.display().to_string()),
                    ("error", e.to_string()),
                ],
            );
            return;
        }
        let stem = target_stem(&source);
        let ext = source
            .extension()
            .map(|e| e.to_string_lossy().to_string())
            .unwrap_or_default();
        let timestamp = Local::now().format("%Y%m%d-%H%M%S%.6f").to_string();
        let name = if ext.is_empty() {
            format!("{stem}_{timestamp}")
        } else {
            format!("{stem}_{timestamp}.{ext}")
        };
        let dest = dest_dir.join(name);
        if std::fs::rename(&source, &dest).is_ok() {
            return;
        }
        match std::fs::copy(&source, &dest) {
            Ok(_) => {
                if let Err(e) = std::fs::remove_file(&source) {
                    self.logger.warn(
                        "no se pudo eliminar el origen tras copiar",
                        &[
                            ("archivo", source.display().to_string()),
                            ("error", e.to_string()),
                        ],
                    );
                }
            }
            Err(e) => self.logger.warn(
                "no se pudo mover el archivo",
                &[
                    ("archivo", source.display().to_string()),
                    ("destino", dest.display().to_string()),
                    ("error", e.to_string()),
                ],
            ),
        }
    }
}

fn err_http_text(http: u16) -> String {
    if http == 0 {
        "-".to_string()
    } else {
        http.to_string()
    }
}

fn target_stem(path: &Path) -> String {
    path.file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "archivo".to_string())
}

pub fn truncate(value: &str, limit: usize) -> String {
    if value.chars().count() <= limit {
        return value.to_string();
    }
    let mut out: String = value.chars().take(limit).collect();
    out.push('…');
    out
}

fn sha256_file(path: &Path) -> std::io::Result<String> {
    let file = File::open(path)?;
    let mut reader = BufReader::new(file);
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 64 * 1024];
    loop {
        let n = reader.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    let digest = hasher.finalize();
    let mut out = String::with_capacity(64);
    for byte in digest {
        out.push_str(&format!("{byte:02x}"));
    }
    Ok(out)
}
