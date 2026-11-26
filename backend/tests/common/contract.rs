//! Vertragsprüfung (Issue #5, RFC-0003): Jede Antwort, die ein API-Test vom Router bekommt, wird gegen
//! `docs/api/openapi.yaml` geprüft – Operation (Methode + Pfadvorlage), dokumentierter Statuscode,
//! Medientyp und JSON-Body gegen das Antwortschema.
//!
//! Antworten des Browserschutzes (403, 415) und die Router-Rückfälle (404/405 für unbekannte Pfade)
//! stehen laut Vertrag nicht an jeder Operation; sie werden gegen `components/responses` bzw. das
//! Fehlerschema geprüft.

use std::collections::HashMap;
use std::sync::{Arc, LazyLock, Mutex};

use axum::Router;
use axum::body::{Body, Bytes};
use axum::http::{HeaderMap, Method, Request, StatusCode, header};
use http_body_util::BodyExt;
use jsonschema::Validator;
use serde_json::{Value, json};
use tower::ServiceExt;

const PREFIX: &str = "/api/v1";

pub struct Contract {
    spec: Value,
    /// (Pfadvorlage, Segmente) aller Pfade unter `paths`.
    paths: Vec<(String, Vec<String>)>,
    validators: Mutex<HashMap<String, Arc<Validator>>>,
}

pub static CONTRACT: LazyLock<Contract> = LazyLock::new(Contract::load);

/// Schickt die Anfrage an den Router, sammelt die Antwort und prüft sie gegen den Vertrag.
pub async fn call(app: &Router, req: Request<Body>) -> (StatusCode, HeaderMap, Bytes) {
    let method = req.method().clone();
    let uri = req.uri().to_string();
    let res = app.clone().oneshot(req).await.unwrap();
    let status = res.status();
    let headers = res.headers().clone();
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    CONTRACT.check(&method, &uri, status, &headers, &bytes);
    (status, headers, bytes)
}

/// Wie [`call`], Body als JSON (`Null`, wenn leer oder kein JSON).
pub async fn call_json(app: &Router, req: Request<Body>) -> (StatusCode, Value) {
    let (status, _, bytes) = call(app, req).await;
    (status, serde_json::from_slice(&bytes).unwrap_or(Value::Null))
}

fn escape(segment: &str) -> String {
    segment.replace('~', "~0").replace('/', "~1")
}

impl Contract {
    pub fn load() -> Contract {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../docs/api/openapi.yaml");
        let text = std::fs::read_to_string(path).expect("openapi.yaml lesbar");
        let spec: Value = serde_yaml_ng::from_str(&text).expect("openapi.yaml ist gültiges YAML");
        let paths = spec["paths"]
            .as_object()
            .expect("paths")
            .keys()
            .map(|p| (p.clone(), p.trim_start_matches('/').split('/').map(str::to_string).collect()))
            .collect();
        Contract { spec, paths, validators: Mutex::new(HashMap::new()) }
    }

    pub fn spec(&self) -> &Value {
        &self.spec
    }

    /// Pfadvorlage zu einem konkreten Pfad (ohne Präfix). Feste Segmente gehen vor Parametern
    /// (`/activities/summary` vor `/activities/{id}`).
    fn template(&self, path: &str) -> Option<&str> {
        let parts: Vec<&str> = path.trim_start_matches('/').split('/').collect();
        self.paths
            .iter()
            .filter(|(_, segs)| {
                segs.len() == parts.len()
                    && segs.iter().zip(&parts).all(|(s, p)| s == p || (s.starts_with('{') && s.ends_with('}') && !p.is_empty()))
            })
            .max_by_key(|(_, segs)| {
                // Bewertung von links: ein festes Segment früher im Pfad schlägt einen Parameter.
                segs.iter().fold(0u64, |acc, s| acc * 2 + u64::from(!s.starts_with('{')))
            })
            .map(|(t, _)| t.as_str())
    }

    /// Folgt einem lokalen `$ref` (`#/components/...`).
    fn deref<'a>(&'a self, mut v: &'a Value) -> &'a Value {
        while let Some(r) = v.get("$ref").and_then(Value::as_str) {
            let pointer = r.strip_prefix('#').unwrap_or_else(|| panic!("nur lokale $ref: {r}"));
            v = self.spec.pointer(pointer).unwrap_or_else(|| panic!("$ref {r} nicht gefunden"));
        }
        v
    }

    /// Validator für das Schema unter `pointer` (JSON-Pointer in die Spec). `#/components/...` in
    /// dem Schema löst gegen die mitgegebenen Komponenten auf.
    pub fn validator(&self, pointer: &str) -> Arc<Validator> {
        if let Some(v) = self.validators.lock().unwrap().get(pointer) {
            return v.clone();
        }
        let schema = self.spec.pointer(pointer).unwrap_or_else(|| panic!("Schema {pointer} fehlt"));
        let doc = json!({
            "$schema": "https://json-schema.org/draft/2020-12/schema",
            "components": self.spec["components"],
            "allOf": [schema],
        });
        let validator = jsonschema::options()
            .with_draft(jsonschema::Draft::Draft202012)
            .should_validate_formats(true)
            .build(&doc)
            .unwrap_or_else(|e| panic!("Schema {pointer} lässt sich nicht übersetzen: {e}"));
        let validator = Arc::new(validator);
        self.validators.lock().unwrap().insert(pointer.to_string(), validator.clone());
        validator
    }

    /// Prüft `value` gegen das Schema unter `pointer`; Fehlermeldungen als Liste.
    pub fn errors(&self, pointer: &str, value: &Value) -> Vec<String> {
        self.validator(pointer).iter_errors(value).map(|e| format!("{} bei '{}'", e, e.instance_path())).collect()
    }

    /// Pointer der Antwort (Response-Objekt) für Operation + Status, `$ref` aufgelöst.
    fn response_pointer(&self, template: Option<&str>, method: &Method, status: StatusCode) -> Result<String, String> {
        let generic = match status.as_u16() {
            403 => Some("/components/responses/Forbidden"),
            415 => Some("/components/responses/UnsupportedMediaType"),
            _ => None,
        };
        let Some(template) = template else {
            // Unbekannter Pfad: nur die Rückfälle des Routers sind erlaubt.
            return match status.as_u16() {
                403 | 415 => Ok(generic.unwrap().to_string()),
                404 | 405 => Ok(String::new()),
                _ => Err(format!("Pfad nicht im Vertrag (Status {status})")),
            };
        };
        let op_pointer = format!("/paths/{}/{}", escape(template), method.as_str().to_ascii_lowercase());
        let Some(op) = self.spec.pointer(&op_pointer) else {
            return if status == StatusCode::METHOD_NOT_ALLOWED || generic.is_some() {
                Ok(generic.map(str::to_string).unwrap_or_default())
            } else {
                Err(format!("Methode {method} für {template} nicht im Vertrag (Status {status})"))
            };
        };
        let code = status.as_u16().to_string();
        let range = format!("{}XX", status.as_u16() / 100);
        for key in [code.as_str(), range.as_str(), "default"] {
            if let Some(resp) = op["responses"].get(key) {
                let mut pointer = format!("{op_pointer}/responses/{key}");
                if let Some(r) = resp.get("$ref").and_then(Value::as_str) {
                    pointer = r.trim_start_matches('#').to_string();
                }
                return Ok(pointer);
            }
        }
        match generic {
            Some(p) => Ok(p.to_string()),
            None => Err(format!("Status {status} nicht dokumentiert für {method} {template}")),
        }
    }

    pub fn check(&self, method: &Method, uri: &str, status: StatusCode, headers: &HeaderMap, body: &[u8]) {
        let path = uri.split('?').next().unwrap_or(uri);
        let Some(rest) = path.strip_prefix(PREFIX) else {
            return; // statische Dateien der Oberfläche
        };
        let template = self.template(rest);
        // Abdeckung messen: TRACKCRAFT_CONTRACT_LOG=<datei> sammelt die geprüften Operationen.
        if let (Ok(log), Some(t)) = (std::env::var("TRACKCRAFT_CONTRACT_LOG"), template) {
            use std::io::Write;
            if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(log) {
                // Eine Zeile in einem Schreibvorgang, Tests laufen parallel.
                let line = format!("{} {t} {}\n", method.as_str().to_ascii_lowercase(), status.as_u16());
                let _ = f.write_all(line.as_bytes());
            }
        }
        let what = format!("{method} {uri} → {status}");
        let pointer = self.response_pointer(template, method, status).unwrap_or_else(|e| panic!("Vertrag: {what}: {e}"));
        let content_type =
            headers.get(header::CONTENT_TYPE).and_then(|v| v.to_str().ok()).map(|v| v.split(';').next().unwrap_or("").trim());

        if pointer.is_empty() {
            // Router-Rückfall: Fehlerhülle.
            let value: Value = serde_json::from_slice(body).unwrap_or_else(|_| panic!("Vertrag: {what}: Fehler ohne JSON"));
            let errs = self.errors("/components/schemas/ErrorResponse", &value);
            assert!(errs.is_empty(), "Vertrag: {what}: Fehlerhülle passt nicht:\n  {}", errs.join("\n  "));
            return;
        }

        let response = self.deref(self.spec.pointer(&pointer).unwrap());
        let Some(content) = response.get("content").and_then(Value::as_object) else {
            assert!(body.is_empty(), "Vertrag: {what}: Antwort ohne Inhalt laut Vertrag, Body hat {} Bytes", body.len());
            return;
        };
        let ct = content_type.unwrap_or_else(|| panic!("Vertrag: {what}: Content-Type fehlt"));
        let (media, entry) = content
            .iter()
            .find(|(m, _)| m.as_str() == ct || m.as_str() == "*/*")
            .unwrap_or_else(|| panic!("Vertrag: {what}: Content-Type {ct} nicht im Vertrag ({:?})", content.keys().collect::<Vec<_>>()));
        if !(media.as_str() == "application/json" || media.ends_with("+json")) || entry.get("schema").is_none() {
            return;
        }
        let value: Value = serde_json::from_slice(body).unwrap_or_else(|e| panic!("Vertrag: {what}: kein JSON: {e}"));
        // Pointer auf das Schema im (aufgelösten) Response-Objekt.
        let resp_pointer = match self.spec.pointer(&pointer).and_then(|r| r.get("$ref")).and_then(Value::as_str) {
            Some(r) => r.trim_start_matches('#').to_string(),
            None => pointer.clone(),
        };
        let schema_pointer = format!("{resp_pointer}/content/{}/schema", escape(media));
        let errs = self.errors(&schema_pointer, &value);
        assert!(
            errs.is_empty(),
            "Vertrag: {what}: Antwort passt nicht zu {schema_pointer}:\n  {}\nBody: {}",
            errs.join("\n  "),
            serde_json::to_string(&value).unwrap_or_default().chars().take(2000).collect::<String>()
        );
    }
}
