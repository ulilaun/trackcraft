//! Schnittstellenvertrag `docs/api/openapi.yaml` (Issue #5, RFC-0003): Beispiele passen zu ihren Schemas,
//! und die Vertragsprüfung der API-Tests (tests/common/contract.rs) erkennt Abweichungen.

mod common;

use axum::http::{HeaderMap, HeaderValue, Method, StatusCode, header};
use common::contract::CONTRACT;
use serde_json::{Value, json};

fn is_schema(v: &Value) -> bool {
    ["type", "properties", "$ref", "allOf", "oneOf", "anyOf", "enum", "const"].iter().any(|k| v.get(k).is_some())
}

/// Sammelt (Pointer, Beispiel) für alle Schemas mit `examples` (JSON-Schema-Liste).
fn collect(v: &Value, pointer: &str, out: &mut Vec<(String, Value)>) {
    match v {
        Value::Object(map) => {
            if let Some(Value::Array(examples)) = map.get("examples")
                && is_schema(v)
            {
                out.extend(examples.iter().map(|e| (pointer.to_string(), e.clone())));
            }
            for (k, child) in map {
                collect(child, &format!("{pointer}/{}", k.replace('~', "~0").replace('/', "~1")), out);
            }
        }
        Value::Array(items) => {
            for (i, child) in items.iter().enumerate() {
                collect(child, &format!("{pointer}/{i}"), out);
            }
        }
        _ => {}
    }
}

#[test]
fn all_examples_match_their_schema() {
    let mut examples = Vec::new();
    collect(CONTRACT.spec(), "", &mut examples);
    assert!(examples.len() >= 10, "zu wenige Beispiele gefunden: {}", examples.len());
    let failures: Vec<String> = examples
        .iter()
        .filter_map(|(pointer, example)| {
            let errs = CONTRACT.errors(pointer, example);
            (!errs.is_empty()).then(|| format!("{pointer}:\n    {}", errs.join("\n    ")))
        })
        .collect();
    assert!(failures.is_empty(), "Beispiele passen nicht zum Schema:\n  {}", failures.join("\n  "));
}

#[test]
fn every_operation_has_id_and_documented_success() {
    let mut missing = Vec::new();
    for (path, item) in CONTRACT.spec()["paths"].as_object().unwrap() {
        for (method, op) in item.as_object().unwrap() {
            if !["get", "post", "patch", "put", "delete"].contains(&method.as_str()) {
                continue;
            }
            let responses = op["responses"].as_object().unwrap();
            if op.get("operationId").is_none() || !responses.keys().any(|k| k.starts_with('2')) {
                missing.push(format!("{method} {path}"));
            }
        }
    }
    assert!(missing.is_empty(), "ohne operationId oder 2xx-Antwort: {missing:?}");
}

fn json_headers() -> HeaderMap {
    let mut h = HeaderMap::new();
    h.insert(header::CONTENT_TYPE, HeaderValue::from_static("application/json"));
    h
}

fn example(name: &str) -> Value {
    CONTRACT.spec()["components"]["schemas"][name]["examples"][0].clone()
}

fn check(method: Method, uri: &str, status: StatusCode, body: &Value) -> Result<(), String> {
    let bytes = if body.is_null() { Vec::new() } else { body.to_string().into_bytes() };
    let headers = if body.is_null() { HeaderMap::new() } else { json_headers() };
    std::panic::catch_unwind(|| CONTRACT.check(&method, uri, status, &headers, &bytes))
        .map_err(|e| e.downcast_ref::<String>().cloned().or_else(|| e.downcast_ref::<&str>().map(|s| s.to_string())).unwrap_or_default())
}

#[test]
fn check_accepts_conforming_answers() {
    let id = example("ActivityResponse")["data"]["id"].as_str().unwrap().to_string();
    check(Method::GET, &format!("/api/v1/activities/{id}"), StatusCode::OK, &example("ActivityResponse")).unwrap();
    // Feste Segmente gehen vor Parametern, Abfrageparameter zählen nicht.
    let err = json!({ "error": { "code": "not_found", "message": "Not found" } });
    check(Method::GET, "/api/v1/activities/unbekannt?x=1", StatusCode::NOT_FOUND, &err).unwrap();
    // Router-Rückfall und Browserschutz gelten für jeden Pfad.
    check(Method::GET, "/api/v1/gibt-es-nicht", StatusCode::NOT_FOUND, &err).unwrap();
    let forbidden = json!({ "error": { "code": "host_not_allowed", "message": "Host not allowed" } });
    check(Method::GET, "/api/v1/health", StatusCode::FORBIDDEN, &forbidden).unwrap();
    // Statische Dateien sind nicht Teil des Vertrags.
    check(Method::GET, "/index.html", StatusCode::OK, &Value::Null).unwrap();
}

#[test]
fn check_rejects_deviations() {
    let mut wrong_type = example("ActivityResponse");
    wrong_type["data"]["distance_m"] = json!("viel");
    let e = check(Method::GET, "/api/v1/activities/0198c1f2-6b3a-7c1e-9d44-2f6a1b8e5a10", StatusCode::OK, &wrong_type).unwrap_err();
    assert!(e.contains("/data/distance_m"), "{e}");

    let mut missing = example("ActivityResponse");
    missing["data"].as_object_mut().unwrap().remove("title");
    let e = check(Method::GET, "/api/v1/activities/0198c1f2-6b3a-7c1e-9d44-2f6a1b8e5a10", StatusCode::OK, &missing).unwrap_err();
    assert!(e.contains("title"), "{e}");

    let e = check(Method::GET, "/api/v1/activities/x", StatusCode::IM_A_TEAPOT, &json!({})).unwrap_err();
    assert!(e.contains("nicht dokumentiert"), "{e}");

    let e = check(Method::POST, "/api/v1/health", StatusCode::OK, &json!({})).unwrap_err();
    assert!(e.contains("nicht im Vertrag"), "{e}");

    let bad_error = json!({ "error": { "message": "ohne Code" } });
    let e = check(Method::GET, "/api/v1/gibt-es-nicht", StatusCode::NOT_FOUND, &bad_error).unwrap_err();
    assert!(e.contains("Fehlerhülle"), "{e}");
}

/// Operationen, die im Vertrag stehen, aber im Backend noch fehlen (mit Issue).
const NOT_IMPLEMENTED: &[&str] = &[];

/// Jede Operation des Vertrags erreicht einen Handler: Der Router-Rückfall (`not_found`,
/// `method_not_allowed`) kommt nur für unbekannte Pfade und Methoden.
#[tokio::test]
async fn every_operation_is_routed() {
    use axum::body::Body;
    use axum::http::Request;
    use http_body_util::BodyExt;
    use std::sync::Arc;
    use tower::ServiceExt;
    use trackcraft::api::{self, AppState};

    let dir = common::temp();
    let cfg = common::validated(dir.path());
    let started = trackcraft::startup::prepare(&cfg).await.unwrap();
    let db = started.db.clone();
    drop(started);
    let app = api::router(AppState { cfg: Arc::new(cfg), db: db.clone(), garmin: Default::default(), backup: Default::default() });

    let mut missing = Vec::new();
    for (path, item) in CONTRACT.spec()["paths"].as_object().unwrap() {
        for method in item.as_object().unwrap().keys() {
            let Ok(m) = method.to_ascii_uppercase().parse::<Method>() else { continue };
            if !["GET", "POST", "PATCH", "PUT", "DELETE"].contains(&m.as_str()) {
                continue;
            }
            let concrete: String = path
                .split('/')
                .map(|s| match s {
                    "{id}" => "00000000-0000-7000-8000-000000000000",
                    "{sport_class}" => "cycling",
                    s if s.starts_with('{') => "x",
                    s => s,
                })
                .collect::<Vec<_>>()
                .join("/");
            let op = format!("{} {path}", m.as_str().to_ascii_lowercase());
            let mut req = Request::builder().method(m.clone()).uri(format!("/api/v1{concrete}")).header("host", "trackcraft.test");
            let body = if m == Method::GET || m == Method::DELETE {
                Body::empty()
            } else {
                req = req.header("origin", "https://trackcraft.test").header("content-type", "application/json");
                Body::from("{}")
            };
            let res = app.clone().oneshot(req.body(body).unwrap()).await.unwrap();
            let bytes = res.into_body().collect().await.unwrap().to_bytes();
            let v: Value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
            let code = v["error"]["code"].as_str().unwrap_or("");
            if (code == "not_found" || code == "method_not_allowed") != NOT_IMPLEMENTED.contains(&op.as_str()) {
                missing.push(format!("{op} ({code})"));
            }
        }
    }
    db.close().await;
    assert!(missing.is_empty(), "Vertrag und Router passen nicht zusammen (oder NOT_IMPLEMENTED veraltet):\n  {}", missing.join("\n  "));
}
