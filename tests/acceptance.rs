use base64::Engine;
use sha2::{Digest, Sha256};
use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

const BIN: &str = env!("CARGO_BIN_EXE_oasiscom-importer");

static COUNTER: AtomicU64 = AtomicU64::new(0);

struct Case {
    dir: PathBuf,
}

impl Case {
    fn new(name: &str) -> Self {
        let n = COUNTER.fetch_add(1, Ordering::SeqCst);
        let dir = std::env::temp_dir().join(format!(
            "oasis_rs_test_{}_{}_{}",
            std::process::id(),
            n,
            name
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        Case { dir }
    }

    fn command(&self) -> Command {
        let mut cmd = Command::new(BIN);
        cmd.current_dir(&self.dir);
        cmd.env_clear();
        cmd.env("IS_MOCK", "true");
        cmd.env("MOCK_LATENCY", "0ms");
        cmd.env("WATCH_DIR", self.dir.join("inbox"));
        cmd.env("STATUS_FILE", self.dir.join("status").join("envios.jsonl"));
        cmd.env("LOG_LEVEL", "error");
        cmd
    }

    fn run(&self, args: &[&str]) -> Output {
        self.command().args(args).output().unwrap()
    }

    fn write_xml(&self, name: &str, content: &str) -> PathBuf {
        let path = self.dir.join(name);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(&path, content).unwrap();
        path
    }

    fn status_lines(&self) -> Vec<serde_json::Value> {
        let path = self.dir.join("status").join("envios.jsonl");
        std::fs::read_to_string(&path)
            .unwrap()
            .lines()
            .map(|l| serde_json::from_str(l).unwrap())
            .collect()
    }
}

const XML: &str = r#"<?xml version="1.0"?>
<ROWS><OPERATIONS><OPERATION>
<COMPANYID>1</COMPANYID><DOCUMENTID>PD</DOCUMENTID><NUMBERID>0</NUMBERID>
<CLIENTID>2</CLIENTID><PURCHASEORDER>PEDIDO-001</PURCHASEORDER>
</OPERATION></OPERATIONS></ROWS>"#;

fn stderr(o: &Output) -> String {
    String::from_utf8_lossy(&o.stderr).to_string()
}

fn stdout(o: &Output) -> String {
    String::from_utf8_lossy(&o.stdout).to_string()
}

#[test]
fn version_and_help() {
    let case = Case::new("version");
    let out = case.run(&["--version"]);
    assert!(out.status.success());
    assert_eq!(stdout(&out).trim(), "oasis 0.1.0");
    let out = case.run(&["help"]);
    assert!(out.status.success());
    assert!(stdout(&out).contains("status"));
    let out = case.run(&["nope"]);
    assert_eq!(out.status.code(), Some(2));
    assert!(stderr(&out).contains("comando desconocido"));
}

#[test]
fn invalid_config_exits_2_with_all_credentials() {
    let case = Case::new("config");
    let out = case
        .command()
        .env("IS_MOCK", "false")
        .arg("version")
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2));
    let err = stderr(&out);
    for key in [
        "OASIS_CONTEXT",
        "OASIS_USERNAME",
        "OASIS_PASSWORD",
        "OASIS_COMPANY_CODE",
        "OASIS_PUBLICATION_CODE",
    ] {
        assert!(err.contains(key), "{key} no está en {err}");
    }
}

#[test]
fn mock_send_success_moves_and_records() {
    let case = Case::new("send_ok");
    let path = case.write_xml("inbox/pedido-001.xml", XML);
    let out = case.run(&["send", path.to_str().unwrap()]);
    assert!(out.status.success(), "{}", stderr(&out));

    let events = case.status_lines();
    assert_eq!(events.len(), 1);
    let event = &events[0];
    assert_eq!(event["estado"], "ENVIADO");
    assert_eq!(event["http"], 200);
    assert_eq!(event["mock"], true);
    assert_eq!(event["empresa"], "1");
    assert_eq!(event["documentos"][0]["documento"], "PD");
    assert_eq!(event["documentos"][0]["pedido"], "PEDIDO-001");
    let expected = format!("{:x}", Sha256::digest(XML.as_bytes()));
    assert_eq!(event["hash"], expected);

    assert!(!path.exists());
    let processed = std::fs::read_dir(case.dir.join("inbox/processed"))
        .unwrap()
        .count();
    assert_eq!(processed, 1);
}

#[test]
fn mock_failure_and_invalid_xml_go_to_failed() {
    let case = Case::new("send_fail");
    let fail = case.write_xml("inbox/fail-pedido.xml", XML);
    let out = case.run(&["send", fail.to_str().unwrap()]);
    assert_eq!(out.status.code(), Some(1));
    let events = case.status_lines();
    assert_eq!(events[0]["estado"], "FALLIDO");
    assert_eq!(events[0]["http"], 400);

    let broken = case.write_xml("inbox/roto.xml", "<ROWS>");
    let out = case.run(&["send", broken.to_str().unwrap()]);
    assert_eq!(out.status.code(), Some(1));
    let events = case.status_lines();
    let last = events.last().unwrap();
    assert_eq!(last["estado"], "FALLIDO");
    assert!(last["error"].as_str().unwrap().contains("XML inválido"));
    assert_eq!(
        std::fs::read_dir(case.dir.join("inbox/failed"))
            .unwrap()
            .count(),
        2
    );
}

#[test]
fn empty_and_oversized_files_fail() {
    let case = Case::new("sizes");
    let empty = case.write_xml("inbox/vacio.xml", "");
    let out = case
        .command()
        .env("MAX_FILE_BYTES", "10")
        .args(["send", empty.to_str().unwrap()])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(1));
    let big = case.write_xml("inbox/grande.xml", XML);
    let out = case
        .command()
        .env("MAX_FILE_BYTES", "10")
        .args(["send", big.to_str().unwrap()])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(1));
    let events = case.status_lines();
    assert_eq!(events.len(), 2);
    assert!(events[0]["error"].as_str().unwrap().contains("vacío"));
    assert!(events[1]["error"]
        .as_str()
        .unwrap()
        .contains("demasiado grande"));
    assert_eq!(
        std::fs::read_dir(case.dir.join("inbox/failed"))
            .unwrap()
            .count(),
        2
    );
}

#[test]
fn send_outside_watch_dir_does_not_move() {
    let case = Case::new("outside");
    let outside = case.dir.join("externo.xml");
    std::fs::write(&outside, XML).unwrap();
    let out = case.run(&["send", outside.to_str().unwrap()]);
    assert!(out.status.success());
    assert!(outside.exists());
    assert!(!case.dir.join("inbox/processed").exists());
}

#[test]
fn batch_processes_existing_files() {
    let case = Case::new("batch");
    case.write_xml("inbox/a.xml", XML);
    case.write_xml("inbox/fail-b.xml", XML);
    let out = case
        .command()
        .env("TRIGGER_WATCH", "false")
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(1));
    let events = case.status_lines();
    assert_eq!(events.len(), 2);
    assert_eq!(
        std::fs::read_dir(case.dir.join("inbox/processed"))
            .unwrap()
            .count(),
        1
    );
    assert_eq!(
        std::fs::read_dir(case.dir.join("inbox/failed"))
            .unwrap()
            .count(),
        1
    );
}

#[test]
fn status_filters_json_and_limits() {
    let case = Case::new("status");
    let path = case.dir.join("status");
    std::fs::create_dir_all(&path).unwrap();
    let lines = [
        r#"{"fecha":"2026-10-27T00:17:37-05:00","archivo":"pedido2.xml","hash":"h2","empresa":"1","documentos":[{"documento":"PD","numero":"0","pedido":"PEDIDO-002","cliente":"2"}],"estado":"ENVIADO","http":200,"mock":true,"duracion_ms":10,"mensaje":"{\"status\":\"OK\"}"}"#,
        r#"{"fecha":"2026-10-26T00:17:36-05:00","archivo":"pedido1.xml","hash":"h1","empresa":"1","estado":"FALLIDO","mock":true,"duracion_ms":5,"error":"XML inválido"}"#,
        "linea corrupta",
    ];
    std::fs::write(
        case.dir.join("status/envios.jsonl"),
        format!("{}\n", lines.join("\n")),
    )
    .unwrap();

    let out = case.run(&["status", "--json"]);
    assert!(out.status.success());
    let json_text = stdout(&out);
    let json_lines: Vec<_> = json_text.lines().collect();
    assert_eq!(json_lines.len(), 2);
    assert!(json_lines
        .iter()
        .all(|l| serde_json::from_str::<serde_json::Value>(l).is_ok()));
    assert!(stderr(&out).contains("Registro:"));
    assert!(stderr(&out).contains("advertencia"));

    let out = case.run(&["status", "--json", "--estado", "fallido"]);
    let text = stdout(&out);
    let lines: Vec<_> = text.lines().collect();
    assert_eq!(lines.len(), 1);
    assert!(lines[0].contains("FALLIDO"));

    let out = case.run(&["status", "--json", "--pedido", "pedido-002"]);
    assert_eq!(stdout(&out).lines().count(), 1);

    let out = case.run(&[
        "status",
        "--json",
        "--desde",
        "2026-10-27",
        "--hasta",
        "2026-10-27",
    ]);
    assert_eq!(stdout(&out).lines().count(), 1);

    let out = case.run(&["status", "--json", "--limit", "1"]);
    assert_eq!(stdout(&out).lines().count(), 1);
    assert!(stderr(&out).contains("Mostrando 1 de 2"));

    let out = case.run(&["status", "--json", "--limit", "1", "--archivo", "nada"]);
    assert!(out.status.success());
    assert_eq!(stdout(&out).trim(), "");
    assert!(stderr(&out).contains("Sin registros"));

    let out = case.run(&["status", "--desde", "2026-13-01"]);
    assert_eq!(out.status.code(), Some(2));
    let out = case.run(&["status", "--limit", "-1"]);
    assert_eq!(out.status.code(), Some(2));
    let out = case.run(&["status", "--estado", "OTRO"]);
    assert_eq!(out.status.code(), Some(2));
}

#[test]
fn status_without_history_is_empty() {
    let case = Case::new("status_empty");
    let out = case.run(&["status", "--json"]);
    assert!(out.status.success());
    assert_eq!(stdout(&out).trim(), "");
    assert!(stderr(&out).contains("Sin registros"));
}

#[test]
fn mock_token_is_cached() {
    let case = Case::new("token");
    let out = case.run(&["token"]);
    assert!(out.status.success());
    let token = stdout(&out).trim().to_string();
    assert!(token.starts_with("mock-token-"));
    // El caché es en memoria: otra invocación genera otro token.
    let out2 = case.run(&["token"]);
    assert!(out2.status.success());
    assert_ne!(token, stdout(&out2).trim());
}

// ---------------------------------------------------------------------------
// Servidor HTTP stub para el flujo real
// ---------------------------------------------------------------------------

struct Request {
    path: String,
    headers: String,
    body: Vec<u8>,
}

struct Reply {
    status: u16,
    body: String,
}

fn spawn_server<F>(
    expected: usize,
    handler: F,
) -> (
    String,
    Arc<Mutex<Vec<Request>>>,
    std::thread::JoinHandle<()>,
)
where
    F: Fn(usize, &Request) -> Reply + Send + 'static,
{
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let requests = Arc::new(Mutex::new(Vec::new()));
    let captured = Arc::clone(&requests);
    let handle = std::thread::spawn(move || {
        for index in 0..expected {
            let (mut stream, _) = listener.accept().unwrap();
            let req = read_request(&mut stream);
            let reply = handler(index, &req);
            captured.lock().unwrap().push(req);
            let body = reply.body.as_bytes();
            let response = format!(
                "HTTP/1.1 {} X\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                reply.status,
                body.len()
            );
            stream.write_all(response.as_bytes()).unwrap();
            stream.write_all(body).unwrap();
            stream.flush().unwrap();
        }
    });
    (format!("http://{addr}"), requests, handle)
}

fn read_request(stream: &mut std::net::TcpStream) -> Request {
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    let mut buf = Vec::new();
    let mut tmp = [0u8; 4096];
    let header_end = loop {
        let n = stream.read(&mut tmp).unwrap();
        assert!(n > 0, "conexión cerrada antes de los headers");
        buf.extend_from_slice(&tmp[..n]);
        if let Some(pos) = find_bytes(&buf, b"\r\n\r\n") {
            break pos + 4;
        }
    };
    let header_text = String::from_utf8_lossy(&buf[..header_end]).to_string();
    let path = header_text
        .lines()
        .next()
        .unwrap()
        .split_whitespace()
        .nth(1)
        .unwrap()
        .to_string();
    let content_length = header_text
        .lines()
        .find_map(|l| {
            let l = l.to_lowercase();
            l.strip_prefix("content-length:")
                .map(|v| v.trim().parse::<usize>().unwrap())
        })
        .unwrap_or(0);
    while buf.len() < header_end + content_length {
        let n = stream.read(&mut tmp).unwrap();
        assert!(n > 0);
        buf.extend_from_slice(&tmp[..n]);
    }
    Request {
        path,
        headers: header_text,
        body: buf[header_end..header_end + content_length].to_vec(),
    }
}

fn find_bytes(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

fn send_real(case: &Case, base: &str, extra: &[(&str, &str)], file: &Path) -> Output {
    let mut cmd = Command::new(BIN);
    cmd.current_dir(&case.dir);
    cmd.env_clear();
    cmd.env("LOG_LEVEL", "error")
        .env("WATCH_DIR", case.dir.join("inbox"))
        .env("STATUS_FILE", case.dir.join("status/envios.jsonl"))
        .env("HTTP_RETRY_BACKOFF", "0ms")
        .env("MOCK_LATENCY", "0ms")
        .env("IS_MOCK", "false")
        .env("OASIS_TOKEN_URL", format!("{base}/connect/token"))
        .env("OASIS_API_URL", format!("{base}/api/externalservices"))
        .env("OASIS_CONTEXT", "CTX")
        .env("OASIS_USERNAME", "user")
        .env("OASIS_PASSWORD", "pass")
        .env("OASIS_COMPANY_CODE", "1")
        .env("OASIS_PUBLICATION_CODE", "PUB")
        .args(["send", file.to_str().unwrap()]);
    for (k, v) in extra {
        cmd.env(k, v);
    }
    cmd.output().unwrap()
}

#[test]
fn real_token_and_multipart_contract() {
    let (base, requests, handle) = spawn_server(2, move |index, _req| {
        if index == 0 {
            Reply {
                status: 200,
                body: r#"{"access_token":"tok1","expires_in":3600,"token_type":"Bearer"}"#.into(),
            }
        } else {
            Reply {
                status: 200,
                body: r#"{"ok":true}"#.into(),
            }
        }
    });
    let case = Case::new("real_multipart");
    let file = case.dir.join("inbox").join("pedido.xml");
    std::fs::create_dir_all(case.dir.join("inbox")).unwrap();
    std::fs::write(&file, XML).unwrap();

    let out = send_real(&case, &base, &[], &file);
    handle.join().unwrap();
    assert!(out.status.success(), "{}", stderr(&out));
    let reqs = requests.lock().unwrap();
    assert_eq!(reqs.len(), 2);
    assert!(reqs[0].path.contains("/connect/token?context=CTX"));
    let token_body = String::from_utf8_lossy(&reqs[0].body);
    for field in [
        "UserName=user",
        "Password=pass",
        "client_id=",
        "client_secret=",
        "scope=OasisCo",
        "grant_type=password",
    ] {
        assert!(token_body.contains(field), "falta {field} en {token_body}");
    }
    assert!(reqs[0]
        .headers
        .to_lowercase()
        .contains("content-type: application/x-www-form-urlencoded"));
    assert_eq!(reqs[1].path, "/api/externalservices/CTX/1/PUB");
    assert!(reqs[1]
        .headers
        .to_lowercase()
        .contains("content-type: multipart/form-data; boundary="));
    let upload_body = String::from_utf8_lossy(&reqs[1].body);
    assert!(upload_body.contains("Content-Disposition: form-data; name=\"token\"\r\n\r\ntok1"));
    let encoded = base64::engine::general_purpose::STANDARD.encode(XML.as_bytes());
    assert!(upload_body.contains(&format!(
        "Content-Disposition: form-data; name=\"FileBase64\"\r\n\r\n{encoded}"
    )));
    assert!(!upload_body.contains("filename="));

    let events = case.status_lines();
    assert_eq!(events[0]["estado"], "ENVIADO");
    assert_eq!(events[0]["http"], 200);
    assert_eq!(events[0]["mock"], false);
}

#[test]
fn real_urlencoded_contract() {
    let (base, requests, handle) = spawn_server(2, |index, _req| Reply {
        status: 200,
        body: if index == 0 {
            r#"{"access_token":"tok1","expires_in":3600}"#.into()
        } else {
            r#"{"ok":true}"#.into()
        },
    });
    let case = Case::new("real_urlencoded");
    let file = case.dir.join("inbox").join("pedido.xml");
    std::fs::create_dir_all(case.dir.join("inbox")).unwrap();
    std::fs::write(&file, XML).unwrap();

    let out = send_real(
        &case,
        &base,
        &[("OASIS_REQUEST_FORMAT", "urlencoded")],
        &file,
    );
    handle.join().unwrap();
    assert!(out.status.success(), "{}", stderr(&out));
    let reqs = requests.lock().unwrap();
    let body = String::from_utf8_lossy(&reqs[1].body);
    assert!(reqs[1]
        .headers
        .to_lowercase()
        .contains("content-type: application/x-www-form-urlencoded"));
    assert!(body.starts_with("token=tok1&FileBase64="));
}

#[test]
fn real_401_refreshes_token_once() {
    let (base, requests, handle) = spawn_server(4, |index, req| {
        match (index, req.path.contains("connect/token")) {
            (0, true) => Reply {
                status: 200,
                body: r#"{"access_token":"tok1","expires_in":3600}"#.into(),
            },
            (1, false) => Reply {
                status: 401,
                body: "no autorizado".into(),
            },
            (2, true) => Reply {
                status: 200,
                body: r#"{"access_token":"tok2","expires_in":3600}"#.into(),
            },
            _ => Reply {
                status: 200,
                body: r#"{"ok":true}"#.into(),
            },
        }
    });
    let case = Case::new("real_401");
    let file = case.dir.join("inbox").join("pedido.xml");
    std::fs::create_dir_all(case.dir.join("inbox")).unwrap();
    std::fs::write(&file, XML).unwrap();

    let out = send_real(&case, &base, &[], &file);
    handle.join().unwrap();
    assert!(out.status.success(), "{}", stderr(&out));
    let reqs = requests.lock().unwrap();
    assert_eq!(reqs.len(), 4);
    let last_upload = String::from_utf8_lossy(&reqs[3].body);
    assert!(last_upload.contains("tok2"));
    assert!(!last_upload.contains("tok1"));
}

#[test]
fn real_400_is_not_retried_and_5xx_is() {
    let (base, requests, handle) = spawn_server(2, |index, _req| {
        if index == 0 {
            Reply {
                status: 200,
                body: r#"{"access_token":"tok1","expires_in":3600}"#.into(),
            }
        } else {
            Reply {
                status: 400,
                body: "rechazo lógico".into(),
            }
        }
    });
    let case = Case::new("real_400");
    let file = case.dir.join("inbox").join("pedido.xml");
    std::fs::create_dir_all(case.dir.join("inbox")).unwrap();
    std::fs::write(&file, XML).unwrap();

    let out = send_real(&case, &base, &[("HTTP_RETRIES", "2")], &file);
    handle.join().unwrap();
    assert_eq!(out.status.code(), Some(1));
    let reqs = requests.lock().unwrap();
    assert_eq!(reqs.len(), 2, "un 400 no debe reintentarse");
    drop(reqs);
    let events = case.status_lines();
    assert_eq!(events[0]["http"], 400);
    assert_eq!(events[0]["mensaje"], "rechazo lógico");

    let (base, requests, handle) = spawn_server(3, |index, _req| {
        if index == 0 {
            Reply {
                status: 200,
                body: r#"{"access_token":"tok1","expires_in":3600}"#.into(),
            }
        } else {
            Reply {
                status: 500,
                body: format!("fallo {index}"),
            }
        }
    });
    let case = Case::new("real_500");
    let file = case.dir.join("inbox").join("pedido.xml");
    std::fs::create_dir_all(case.dir.join("inbox")).unwrap();
    std::fs::write(&file, XML).unwrap();
    let out = send_real(&case, &base, &[("HTTP_RETRIES", "1")], &file);
    handle.join().unwrap();
    assert_eq!(out.status.code(), Some(1));
    let reqs = requests.lock().unwrap();
    assert_eq!(reqs.len(), 3);
    let events = case.status_lines();
    assert_eq!(events[0]["estado"], "FALLIDO");
    assert_eq!(events[0]["http"], 500);
    assert_eq!(events[0]["mensaje"], "fallo 2");
}

#[test]
fn watcher_ignores_existing_and_waits_for_stability() {
    let case = Case::new("watch_stable");
    let existing = case.write_xml("inbox/viejo.xml", XML);
    let mut child = case
        .command()
        .env("TRIGGER_WATCH", "true")
        .env("WATCH_PROCESS_EXISTING", "false")
        .env("WATCH_STABLE_CHECKS", "2")
        .env("WATCH_POLL_INTERVAL", "100ms")
        .arg("watch")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();

    std::thread::sleep(Duration::from_millis(450));
    assert!(existing.exists(), "un archivo previo no debe procesarse");
    assert!(case
        .dir
        .join("inbox/processed")
        .read_dir()
        .map(|mut d| d.next().is_none())
        .unwrap_or(true));

    let new_file = case.dir.join("inbox/nuevo.xml");
    std::fs::write(&new_file, XML).unwrap();
    let deadline = Instant::now() + Duration::from_secs(8);
    let mut moved = false;
    while Instant::now() < deadline {
        if case
            .dir
            .join("inbox/processed")
            .read_dir()
            .map(|mut d| d.next().is_some())
            .unwrap_or(false)
        {
            moved = true;
            break;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    let _ = Command::new("kill")
        .arg("-TERM")
        .arg(child.id().to_string())
        .status();
    let output = child.wait().unwrap();
    assert!(moved, "el archivo nuevo no se procesó tras estabilizarse");
    assert!(output.success());
    assert!(existing.exists());
    let events = case.status_lines();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0]["archivo"], "nuevo.xml");
}

#[test]
fn watcher_processes_new_file_and_stops_with_sigterm() {
    let case = Case::new("watch");
    std::fs::create_dir_all(case.dir.join("inbox")).unwrap();
    let mut child = case
        .command()
        .env("TRIGGER_WATCH", "true")
        .env("WATCH_PROCESS_EXISTING", "true")
        .env("WATCH_STABLE_CHECKS", "1")
        .env("WATCH_POLL_INTERVAL", "100ms")
        .arg("watch")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();

    let file = case.dir.join("inbox/tarde.xml");
    std::fs::write(&file, XML).unwrap();

    let deadline = Instant::now() + Duration::from_secs(8);
    let mut moved = false;
    while Instant::now() < deadline {
        if case
            .dir
            .join("inbox/processed")
            .read_dir()
            .map(|mut d| d.next().is_some())
            .unwrap_or(false)
        {
            moved = true;
            break;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    let status = Command::new("kill")
        .arg("-TERM")
        .arg(child.id().to_string())
        .status()
        .unwrap();
    assert!(status.success());
    let output = child.wait().unwrap();
    assert!(moved, "el archivo no llegó a processed/");
    assert!(output.success(), "SIGTERM debe salir con 0");
    let events = case.status_lines();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0]["estado"], "ENVIADO");
}
