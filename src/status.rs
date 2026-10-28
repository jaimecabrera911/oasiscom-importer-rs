use crate::xml::Document;
use chrono::{DateTime, FixedOffset};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs::OpenOptions;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Event {
    #[serde(default)]
    pub fecha: String,
    #[serde(default)]
    pub archivo: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub hash: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub empresa: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub documentos: Vec<Document>,
    #[serde(default)]
    pub estado: String,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub http: u16,
    #[serde(default)]
    pub mock: bool,
    #[serde(default)]
    pub duracion_ms: u64,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub mensaje: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub error: String,
}

fn is_zero(v: &u16) -> bool {
    *v == 0
}

pub struct StatusStore {
    path: PathBuf,
    enabled: bool,
    lock: Mutex<()>,
}

impl StatusStore {
    pub fn new(path: PathBuf, enabled: bool) -> Self {
        StatusStore {
            path,
            enabled,
            lock: Mutex::new(()),
        }
    }

    pub fn append(&self, event: &Event) -> io::Result<()> {
        if !self.enabled {
            return Ok(());
        }
        let _guard = self.lock.lock().expect("lock envenenado");
        if let Some(parent) = self.path.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent)?;
            }
        }
        let mut line = serde_json::to_string(event).map_err(io::Error::other)?;
        line.push('\n');
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)?;
        file.write_all(line.as_bytes())?;
        file.sync_all()?;
        Ok(())
    }
}

pub struct ReadResult {
    pub events: Vec<Event>,
    pub invalid: usize,
}

pub fn read_events(path: &Path) -> io::Result<ReadResult> {
    let content = match std::fs::read_to_string(path) {
        Ok(c) => c,
        Err(e) if e.kind() == io::ErrorKind::NotFound => {
            return Ok(ReadResult {
                events: Vec::new(),
                invalid: 0,
            })
        }
        Err(e) => return Err(e),
    };
    let mut events = Vec::new();
    let mut invalid = 0;
    for line in content.lines() {
        if line.trim().is_empty() {
            continue;
        }
        match serde_json::from_str::<Event>(line) {
            Ok(event) => events.push(event),
            Err(_) => invalid += 1,
        }
    }
    Ok(ReadResult { events, invalid })
}

#[derive(Debug, Default)]
pub struct Summary {
    pub total: usize,
    pub enviados: usize,
    pub fallidos: usize,
    pub archivos_unicos: usize,
    pub con_mas_de_un_intento: usize,
    pub ultimo: Option<Event>,
}

pub fn fecha_key(event: &Event) -> Option<DateTime<FixedOffset>> {
    DateTime::parse_from_rfc3339(&event.fecha).ok()
}

pub fn summarize(events: &[Event]) -> Summary {
    let mut hashes: HashMap<&str, usize> = HashMap::new();
    let mut enviados = 0;
    let mut fallidos = 0;
    for event in events {
        match event.estado.as_str() {
            "ENVIADO" => enviados += 1,
            "FALLIDO" => fallidos += 1,
            _ => {}
        }
        if !event.hash.is_empty() {
            *hashes.entry(event.hash.as_str()).or_insert(0) += 1;
        }
    }
    let ultimo = events
        .iter()
        .filter_map(|e| fecha_key(e).map(|f| (f, e)))
        .max_by_key(|(f, _)| *f)
        .map(|(_, e)| e.clone());
    Summary {
        total: events.len(),
        enviados,
        fallidos,
        archivos_unicos: hashes.len(),
        con_mas_de_un_intento: hashes.values().filter(|n| **n > 1).count(),
        ultimo,
    }
}

pub fn sorted_desc(events: &[Event]) -> Vec<Event> {
    let mut out = events.to_vec();
    out.sort_by(|a, b| {
        let ka = fecha_key(a);
        let kb = fecha_key(b);
        kb.cmp(&ka)
    });
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn event(estado: &str, hash: &str, fecha: &str) -> Event {
        Event {
            estado: estado.into(),
            hash: hash.into(),
            fecha: fecha.into(),
            archivo: "a.xml".into(),
            mock: true,
            duracion_ms: 1,
            ..Default::default()
        }
    }

    #[test]
    fn appends_json_lines_and_summarizes() {
        let dir = std::env::temp_dir().join(format!("oasis_status_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let path = dir.join("sub").join("envios.jsonl");
        let store = StatusStore::new(path.clone(), true);
        store
            .append(&event("ENVIADO", "h1", "2026-10-27T00:00:00-05:00"))
            .unwrap();
        store
            .append(&event("FALLIDO", "h1", "2026-10-27T00:01:00-05:00"))
            .unwrap();
        store
            .append(&event("ENVIADO", "h2", "2026-10-27T00:02:00-05:00"))
            .unwrap();

        let read = read_events(&path).unwrap();
        assert_eq!(read.events.len(), 3);
        let s = summarize(&read.events);
        assert_eq!((s.total, s.enviados, s.fallidos), (3, 2, 1));
        assert_eq!(s.archivos_unicos, 2);
        assert_eq!(s.con_mas_de_un_intento, 1);
        assert_eq!(s.ultimo.unwrap().hash, "h2");
        assert_eq!(sorted_desc(&read.events)[0].hash, "h2");
    }

    #[test]
    fn skips_invalid_lines_and_missing_file() {
        let dir = std::env::temp_dir().join(format!("oasis_status_bad_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("envios.jsonl");
        std::fs::write(&path, "no json\n\n{\"estado\":\"ENVIADO\"}\n").unwrap();
        let read = read_events(&path).unwrap();
        assert_eq!(read.events.len(), 1);
        assert_eq!(read.invalid, 1);
        assert_eq!(
            read_events(&dir.join("nope.jsonl")).unwrap().events.len(),
            0
        );
    }
}
