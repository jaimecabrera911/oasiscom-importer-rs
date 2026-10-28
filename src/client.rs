use crate::config::{Config, RequestFormat};
use crate::logger::Logger;
use base64::Engine;
use chrono::{Local, SecondsFormat};
use reqwest::blocking::multipart::Form;
use reqwest::blocking::{Client as HttpClient, Response};
use reqwest::Url;
use serde_json::json;
use std::io::Read;
use std::sync::Mutex;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const TOKEN_BODY_LIMIT: u64 = 1024 * 1024;
const UPLOAD_BODY_LIMIT: u64 = 8 * 1024 * 1024;
const TOKEN_ERROR_BODY_LIMIT: usize = 500;
const MOCK_TOKEN_TTL: Duration = Duration::from_secs(300);

#[derive(Debug)]
pub struct SendError {
    pub message: String,
    pub http: u16,
    pub body: String,
}

impl SendError {
    fn new(message: impl Into<String>, http: u16, body: impl Into<String>) -> Self {
        SendError {
            message: message.into(),
            http,
            body: body.into(),
        }
    }
}

pub struct UploadResult {
    pub http: u16,
    pub body: String,
}

struct CachedToken {
    token: String,
    expires_at: Instant,
}

pub struct Client<'a> {
    cfg: &'a Config,
    logger: &'a Logger,
    http: HttpClient,
    cached: Mutex<Option<CachedToken>>,
}

fn truncate_chars(value: &str, limit: usize) -> String {
    if value.chars().count() <= limit {
        return value.to_string();
    }
    let mut out: String = value.chars().take(limit).collect();
    out.push('…');
    out
}

fn retryable(status: u16) -> bool {
    status == 429 || status >= 500
}

impl<'a> Client<'a> {
    pub fn new(cfg: &'a Config, logger: &'a Logger) -> Result<Self, String> {
        let http = HttpClient::builder()
            .timeout(cfg.http_timeout)
            .build()
            .map_err(|e| format!("no se pudo crear el cliente HTTP: {e}"))?;
        Ok(Client {
            cfg,
            logger,
            http,
            cached: Mutex::new(None),
        })
    }

    pub fn token(&self) -> Result<String, SendError> {
        {
            let guard = self.cached.lock().expect("lock envenenado");
            if let Some(cached) = guard.as_ref() {
                if cached.expires_at > Instant::now() {
                    return Ok(cached.token.clone());
                }
            }
        }
        let (token, ttl) = if self.cfg.is_mock {
            self.mock_token()
        } else {
            self.real_token()?
        };
        let margin = if ttl <= Duration::from_secs(120) {
            ttl / 4
        } else {
            Duration::from_secs(60)
        };
        let mut guard = self.cached.lock().expect("lock envenenado");
        *guard = Some(CachedToken {
            token: token.clone(),
            expires_at: Instant::now() + ttl.saturating_sub(margin),
        });
        Ok(token)
    }

    pub fn invalidate(&self) {
        let mut guard = self.cached.lock().expect("lock envenenado");
        *guard = None;
    }

    fn mock_token(&self) -> (String, Duration) {
        std::thread::sleep(self.cfg.mock_latency);
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        (format!("mock-token-{nanos}"), MOCK_TOKEN_TTL)
    }

    fn real_token(&self) -> Result<(String, Duration), SendError> {
        let mut url = Url::parse(&self.cfg.token_url)
            .map_err(|e| SendError::new(format!("OASIS_TOKEN_URL inválida: {e}"), 0, ""))?;
        url.query_pairs_mut()
            .append_pair("context", &self.cfg.context);

        let total = self.cfg.http_retries as u64 + 1;
        let mut attempt: u64 = 1;
        loop {
            let response = self
                .http
                .post(url.clone())
                .header("Accept", "application/json")
                .form(&[
                    ("UserName", self.cfg.username.as_str()),
                    ("Password", self.cfg.password.as_str()),
                    ("client_id", self.cfg.client_id.as_str()),
                    ("client_secret", self.cfg.client_secret.as_str()),
                    ("scope", self.cfg.scope.as_str()),
                    ("grant_type", self.cfg.grant_type.as_str()),
                ])
                .send();

            let (status, body) = match response {
                Ok(resp) => match read_limited(resp, TOKEN_BODY_LIMIT) {
                    Ok(pair) => pair,
                    Err(e) => {
                        let err = SendError::new(
                            format!("error leyendo respuesta del token: {e}"),
                            0,
                            "",
                        );
                        if attempt < total {
                            self.wait(attempt);
                            attempt += 1;
                            continue;
                        }
                        return Err(err);
                    }
                },
                Err(e) => {
                    let err =
                        SendError::new(format!("error de red al solicitar token: {e}"), 0, "");
                    if attempt < total {
                        self.wait(attempt);
                        attempt += 1;
                        continue;
                    }
                    return Err(err);
                }
            };

            if (200..300).contains(&status) {
                let value: serde_json::Value = serde_json::from_str(&body).map_err(|e| {
                    SendError::new(
                        format!("respuesta de token no es JSON válido: {e}"),
                        status,
                        "",
                    )
                })?;
                let token = value
                    .get("access_token")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                if token.is_empty() {
                    let err = value.get("error").and_then(|v| v.as_str()).unwrap_or("");
                    let desc = value
                        .get("error_description")
                        .and_then(|v| v.as_str())
                        .unwrap_or("");
                    let mut message = "respuesta de token sin access_token".to_string();
                    if !err.is_empty() {
                        message.push_str(&format!(" (error: {err})"));
                    }
                    if !desc.is_empty() {
                        message.push_str(&format!(": {desc}"));
                    }
                    return Err(SendError::new(message, status, body));
                }
                let ttl = value
                    .get("expires_in")
                    .and_then(|v| v.as_u64())
                    .filter(|v| *v > 0)
                    .map(Duration::from_secs)
                    .unwrap_or(Duration::from_secs(300));
                return Ok((token, ttl));
            }

            if retryable(status) && attempt < total {
                self.wait(attempt);
                attempt += 1;
                continue;
            }
            return Err(SendError::new(
                format!("error HTTP {status} al solicitar token"),
                status,
                truncate_chars(&body, TOKEN_ERROR_BODY_LIMIT),
            ));
        }
    }

    fn wait(&self, attempt: u64) {
        let delay = self.cfg.retry_backoff * attempt as u32;
        self.logger.debug(
            "esperando antes de reintentar",
            &[
                ("intento", attempt.to_string()),
                ("espera", format!("{}ms", delay.as_millis())),
            ],
        );
        std::thread::sleep(delay);
    }

    fn upload_url(&self) -> Result<Url, SendError> {
        let base = self.cfg.api_url.trim_end_matches('/');
        let mut url = Url::parse(base)
            .map_err(|e| SendError::new(format!("OASIS_API_URL inválida: {e}"), 0, ""))?;
        url.path_segments_mut()
            .map_err(|_| SendError::new("OASIS_API_URL no admite segmentos de ruta", 0, ""))?
            .push(&self.cfg.context)
            .push(&self.cfg.company_code)
            .push(&self.cfg.publication_code);
        Ok(url)
    }

    fn send_upload(
        &self,
        url: &Url,
        token: &str,
        base64: &str,
    ) -> Result<Response, reqwest::Error> {
        let request = self
            .http
            .post(url.clone())
            .header("Accept", "application/json");
        match self.cfg.request_format {
            RequestFormat::Multipart => {
                let form = Form::new()
                    .text(self.cfg.token_field.clone(), token.to_string())
                    .text(self.cfg.file_field.clone(), base64.to_string());
                request.multipart(form).send()
            }
            RequestFormat::Urlencoded => request
                .form(&[
                    (self.cfg.token_field.as_str(), token),
                    (self.cfg.file_field.as_str(), base64),
                ])
                .send(),
        }
    }

    pub fn upload(
        &self,
        file_name: &str,
        content: &[u8],
        company: &str,
    ) -> Result<UploadResult, SendError> {
        if self.cfg.is_mock {
            return self.mock_upload(file_name, content, company);
        }

        let base64 = base64::engine::general_purpose::STANDARD.encode(content);
        let url = self.upload_url()?;
        let mut token = self.token()?;
        let total = self.cfg.http_retries as u64 + 1;
        let mut attempt: u64 = 1;
        let mut auth_retried = false;

        loop {
            let response = match self.send_upload(&url, &token, &base64) {
                Ok(resp) => resp,
                Err(e) => {
                    let err = SendError::new(format!("error de red al enviar XML: {e}"), 0, "");
                    if attempt < total {
                        self.wait(attempt);
                        attempt += 1;
                        continue;
                    }
                    return Err(err);
                }
            };
            let (status, body) = match read_limited(response, UPLOAD_BODY_LIMIT) {
                Ok(pair) => pair,
                Err(e) => {
                    return Err(SendError::new(
                        format!("error leyendo respuesta de carga: {e}"),
                        0,
                        "",
                    ))
                }
            };

            if (200..300).contains(&status) {
                return Ok(UploadResult { http: status, body });
            }
            if status == 401 && !auth_retried {
                self.invalidate();
                token = self.token()?;
                auth_retried = true;
                continue;
            }
            if retryable(status) && attempt < total {
                self.wait(attempt);
                attempt += 1;
                continue;
            }
            return Err(SendError::new(
                format!("envío rechazado con HTTP {status}"),
                status,
                body,
            ));
        }
    }

    fn mock_upload(
        &self,
        file_name: &str,
        content: &[u8],
        company: &str,
    ) -> Result<UploadResult, SendError> {
        let text = std::str::from_utf8(content)
            .map_err(|e| SendError::new(format!("XML inválido: {e}"), 0, ""))?;
        roxmltree::Document::parse(text)
            .map_err(|e| SendError::new(format!("XML inválido: {e}"), 0, ""))?;
        std::thread::sleep(self.cfg.mock_latency);

        if self.cfg.mock_fail || file_name.to_lowercase().contains("fail") {
            return Err(SendError::new(
                "envío simulado rechazado (MOCK_FAIL=true o nombre de archivo contiene 'fail')",
                400,
                r#"{"mock":true,"status":"ERROR","message":"Fallo simulado por configuración"}"#,
            ));
        }

        let body = json!({
            "mock": true,
            "status": "OK",
            "message": "Archivo procesado correctamente (simulación, no se envió a OasisCom)",
            "file": file_name,
            "bytes": content.len(),
            "company": company,
            "publication": self.cfg.publication_code,
            "timestamp": Local::now().to_rfc3339_opts(SecondsFormat::Secs, false),
        })
        .to_string();
        Ok(UploadResult { http: 200, body })
    }
}

fn read_limited(resp: Response, limit: u64) -> std::io::Result<(u16, String)> {
    let status = resp.status().as_u16();
    let mut buf = Vec::new();
    resp.take(limit).read_to_end(&mut buf)?;
    Ok((status, String::from_utf8_lossy(&buf).to_string()))
}
