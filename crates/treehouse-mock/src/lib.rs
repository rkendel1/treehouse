use std::{
    collections::BTreeMap,
    io::Read,
    path::Path,
    sync::{Arc, Mutex},
};

use anyhow::{anyhow, Context, Result};
use serde_json::{json, Map, Value};
use tiny_http::{Header, Method, Response, Server, StatusCode};
use treehouse_api::{generate_api_surface, HttpMethod};
use treehouse_graph::{GraphSource, UniversalDataGraph};
use treehouse_parser::parse_structured_file;

pub fn run_mock_server(model_path: &Path) -> Result<()> {
    let parsed = parse_structured_file(model_path)
        .with_context(|| format!("failed to parse model file: {}", model_path.display()))?;
    let source_name = parsed.path.display().to_string();
    let graph = UniversalDataGraph::build(&[GraphSource {
        name: &source_name,
        document: &parsed.document,
    }]);
    let surface = generate_api_surface(&graph);

    if surface.endpoints.is_empty() {
        return Err(anyhow!(
            "no entities discovered in model; could not generate API surface"
        ));
    }

    let mut collections: BTreeMap<String, Vec<Value>> = BTreeMap::new();
    seed_collections(&parsed.document.root().clone(), &mut collections);

    for entity in &surface.entities {
        collections.entry(plural_route(entity)).or_default();
    }

    let state = Arc::new(AppState {
        collections: Mutex::new(collections),
    });

    let server = Server::http("127.0.0.1:4000").context("failed to bind localhost:4000")?;
    println!("Treehouse mock runtime listening on http://localhost:4000");
    for endpoint in &surface.endpoints {
        println!("{} {}", method_name(endpoint.method), endpoint.path);
    }

    for mut request in server.incoming_requests() {
        let method = request.method().clone();
        let path = request.url().to_string();
        let response = handle_request(&state, method, &path, &mut request);
        let _ = request.respond(response);
    }

    Ok(())
}

struct AppState {
    collections: Mutex<BTreeMap<String, Vec<Value>>>,
}

fn handle_request(
    state: &AppState,
    method: Method,
    path: &str,
    request: &mut tiny_http::Request,
) -> Response<std::io::Cursor<Vec<u8>>> {
    let Some((collection, id)) = parse_route(path) else {
        return json_response(StatusCode(404), json!({"error":"route not found"}));
    };

    let mut collections = match state.collections.lock() {
        Ok(guard) => guard,
        Err(_) => {
            return json_response(StatusCode(500), json!({"error":"state lock poisoned"}));
        }
    };
    let entries = collections.entry(collection.clone()).or_default();

    match (method, id) {
        (Method::Get, None) => json_response(StatusCode(200), Value::Array(entries.clone())),
        (Method::Get, Some(id)) => {
            if let Some(found) = entries
                .iter()
                .find(|item| infer_id(item).as_deref() == Some(id.as_str()))
            {
                json_response(StatusCode(200), found.clone())
            } else {
                json_response(StatusCode(404), json!({"error":"resource not found"}))
            }
        }
        (Method::Post, None) => match read_json_body(request) {
            Ok(mut body) => {
                if infer_id(&body).is_none() {
                    let generated = format!("{}", entries.len() + 1);
                    if let Some(map) = body.as_object_mut() {
                        map.insert("id".to_string(), Value::String(generated));
                    }
                }
                entries.push(body.clone());
                json_response(StatusCode(201), body)
            }
            Err(err) => json_response(StatusCode(400), json!({"error": err.to_string()})),
        },
        (Method::Patch, Some(id)) => match read_json_body(request) {
            Ok(body) => {
                if let Some(existing) = entries
                    .iter_mut()
                    .find(|item| infer_id(item).as_deref() == Some(id.as_str()))
                {
                    merge_object(existing, &body);
                    json_response(StatusCode(200), existing.clone())
                } else {
                    json_response(StatusCode(404), json!({"error":"resource not found"}))
                }
            }
            Err(err) => json_response(StatusCode(400), json!({"error": err.to_string()})),
        },
        _ => json_response(StatusCode(405), json!({"error":"method not allowed"})),
    }
}

fn parse_route(path: &str) -> Option<(String, Option<String>)> {
    let trimmed = path.trim_start_matches('/').split('?').next()?;
    if trimmed.is_empty() {
        return None;
    }
    let mut segments = trimmed.split('/');
    let collection = segments.next()?.to_string();
    let id = segments.next().map(|segment| segment.to_string());
    if segments.next().is_some() {
        return None;
    }
    Some((collection, id))
}

fn read_json_body(request: &mut tiny_http::Request) -> Result<Value> {
    let mut body = String::new();
    request
        .as_reader()
        .read_to_string(&mut body)
        .context("failed reading request body")?;
    serde_json::from_str(&body).context("request body is not valid JSON")
}

fn merge_object(target: &mut Value, patch: &Value) {
    if let (Some(target_map), Some(patch_map)) = (target.as_object_mut(), patch.as_object()) {
        for (key, value) in patch_map {
            target_map.insert(key.clone(), value.clone());
        }
    }
}

fn infer_id(value: &Value) -> Option<String> {
    let object = value.as_object()?;
    if let Some(id) = object.get("id").and_then(as_id_string) {
        return Some(id);
    }
    for (key, candidate) in object {
        if key.ends_with("Id") || key.ends_with("_id") || key.ends_with("id") {
            if let Some(id) = as_id_string(candidate) {
                return Some(id);
            }
        }
    }
    None
}

fn as_id_string(value: &Value) -> Option<String> {
    match value {
        Value::String(v) => Some(v.clone()),
        Value::Number(v) => Some(v.to_string()),
        _ => None,
    }
}

fn seed_collections(root: &Value, collections: &mut BTreeMap<String, Vec<Value>>) {
    match root {
        Value::Object(map) => {
            for (key, value) in map {
                if let Value::Array(items) = value {
                    collections.insert(key.to_lowercase(), items.clone());
                }
            }
        }
        Value::Array(items) => {
            collections.insert("items".to_string(), items.clone());
        }
        _ => {}
    }
}

fn plural_route(entity: &str) -> String {
    let base = entity.to_lowercase();
    if base.ends_with('s') {
        base
    } else if base.ends_with('y') && base.len() > 1 {
        format!("{}ies", &base[..base.len() - 1])
    } else {
        format!("{base}s")
    }
}

fn method_name(method: HttpMethod) -> &'static str {
    match method {
        HttpMethod::Get => "GET",
        HttpMethod::Post => "POST",
        HttpMethod::Patch => "PATCH",
    }
}

fn json_response(status: StatusCode, value: Value) -> Response<std::io::Cursor<Vec<u8>>> {
    let body = serde_json::to_vec_pretty(&value).unwrap_or_else(|_| b"{}".to_vec());
    let mut response = Response::from_data(body).with_status_code(status);
    if let Ok(header) = Header::from_bytes("content-type", "application/json") {
        response = response.with_header(header);
    }
    response
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_collection_and_resource_routes() {
        assert_eq!(
            parse_route("/customers"),
            Some(("customers".to_string(), None))
        );
        assert_eq!(
            parse_route("/orders/abc"),
            Some(("orders".to_string(), Some("abc".to_string())))
        );
        assert_eq!(parse_route("/orders/abc/extra"), None);
    }

    #[test]
    fn infers_id_variants() {
        assert_eq!(infer_id(&json!({"id":"1"})).as_deref(), Some("1"));
        assert_eq!(infer_id(&json!({"orderId":"x"})).as_deref(), Some("x"));
        assert_eq!(infer_id(&json!({"name":"none"})), None);
    }
}
