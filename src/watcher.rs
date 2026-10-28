use crate::config::Config;
use crate::logger::Logger;
use crate::processor::Processor;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant, SystemTime};

struct FileState {
    size: u64,
    mtime: SystemTime,
    stable: u32,
    done: bool,
}

pub struct Watcher<'a> {
    cfg: &'a Config,
    processor: &'a Processor<'a>,
    logger: &'a Logger,
}

impl<'a> Watcher<'a> {
    pub fn new(cfg: &'a Config, processor: &'a Processor<'a>, logger: &'a Logger) -> Self {
        Watcher {
            cfg,
            processor,
            logger,
        }
    }

    pub fn run(&self, stop: &AtomicBool) -> i32 {
        for dir in [
            &self.cfg.watch_dir,
            &self.cfg.processed_dir,
            &self.cfg.failed_dir,
        ] {
            if let Err(e) = std::fs::create_dir_all(dir) {
                self.logger.error(
                    "no se pudo crear carpeta",
                    &[
                        ("carpeta", dir.display().to_string()),
                        ("error", e.to_string()),
                    ],
                );
                return 1;
            }
        }
        self.logger.info(
            "iniciando vigilancia",
            &[
                ("carpeta", self.cfg.watch_dir.display().to_string()),
                (
                    "intervalo",
                    format!("{}ms", self.cfg.poll_interval.as_millis()),
                ),
                ("process_existing", self.cfg.process_existing.to_string()),
                (
                    "move_after_process",
                    self.cfg.move_after_process.to_string(),
                ),
                ("conexion", crate::logger::mask_context(&self.cfg.context)),
            ],
        );

        let mut state: HashMap<PathBuf, FileState> = HashMap::new();
        let mut initial = true;
        while !stop.load(Ordering::SeqCst) {
            self.scan(&mut state, initial);
            initial = false;
            if stop.load(Ordering::SeqCst) {
                break;
            }
            self.sleep(stop);
        }
        self.logger.info("vigilancia detenida", &[]);
        0
    }

    fn sleep(&self, stop: &AtomicBool) {
        let deadline = Instant::now() + self.cfg.poll_interval;
        while Instant::now() < deadline {
            if stop.load(Ordering::SeqCst) {
                return;
            }
            let remaining = deadline.saturating_duration_since(Instant::now());
            std::thread::sleep(remaining.min(Duration::from_millis(200)));
        }
    }

    fn scan(&self, state: &mut HashMap<PathBuf, FileState>, initial: bool) {
        let entries = match std::fs::read_dir(&self.cfg.watch_dir) {
            Ok(e) => e,
            Err(e) => {
                if e.kind() == std::io::ErrorKind::NotFound {
                    if let Err(create) = std::fs::create_dir_all(&self.cfg.watch_dir) {
                        self.logger.warn(
                            "no se pudo recrear la carpeta vigilada",
                            &[
                                ("carpeta", self.cfg.watch_dir.display().to_string()),
                                ("error", create.to_string()),
                            ],
                        );
                    }
                } else {
                    self.logger.warn(
                        "error leyendo carpeta vigilada",
                        &[
                            ("carpeta", self.cfg.watch_dir.display().to_string()),
                            ("error", e.to_string()),
                        ],
                    );
                }
                return;
            }
        };

        let mut candidates: Vec<(PathBuf, u64, SystemTime)> = Vec::new();
        for entry in entries {
            let Ok(entry) = entry else { continue };
            let path = entry.path();
            let Ok(file_type) = entry.file_type() else {
                continue;
            };
            if !file_type.is_file() {
                continue;
            }
            let name = entry.file_name().to_string_lossy().to_string();
            if name.starts_with('.') {
                continue;
            }
            let is_xml = path
                .extension()
                .map(|e| e.to_string_lossy().to_lowercase() == "xml")
                .unwrap_or(false);
            if !is_xml {
                continue;
            }
            let metadata = match entry.metadata() {
                Ok(m) => m,
                Err(e) => {
                    self.logger.warn(
                        "no se pudo leer metadata",
                        &[
                            ("archivo", path.display().to_string()),
                            ("error", e.to_string()),
                        ],
                    );
                    continue;
                }
            };
            let mtime = match metadata.modified() {
                Ok(m) => m,
                Err(e) => {
                    self.logger.warn(
                        "no se pudo leer fecha de modificación",
                        &[
                            ("archivo", path.display().to_string()),
                            ("error", e.to_string()),
                        ],
                    );
                    continue;
                }
            };
            candidates.push((path, metadata.len(), mtime));
        }

        let present: std::collections::HashSet<PathBuf> =
            candidates.iter().map(|(p, _, _)| p.clone()).collect();
        state.retain(|p, _| present.contains(p));

        candidates.sort_by(|a, b| a.2.cmp(&b.2).then_with(|| a.0.cmp(&b.0)));

        for (path, size, mtime) in candidates {
            let is_new = !state.contains_key(&path);
            let entry = state.entry(path.clone()).or_insert_with(|| FileState {
                size,
                mtime,
                stable: 1,
                done: initial && !self.cfg.process_existing,
            });
            if !is_new {
                if entry.size == size && entry.mtime == mtime {
                    entry.stable = entry.stable.saturating_add(1);
                } else {
                    entry.size = size;
                    entry.mtime = mtime;
                    entry.stable = 1;
                }
            }
            if entry.done {
                continue;
            }
            if entry.stable < self.cfg.stable_checks {
                continue;
            }
            entry.done = true;
            self.processor.process_file(&path);
        }
    }
}
