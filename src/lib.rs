//! Counters in Redis, through Cosmonic Desktop's `cache` binding.
//!
//! ```text
//! GET    /                              the web UI
//! GET    /api/counters                  every counter: {"counters":[{"name","value"}]}
//! POST   /api/counters/<name>/increment +1 (creates it at 1)
//! POST   /api/counters/<name>/decrement -1 (not below 0)
//! DELETE /api/counters/<name>           remove it
//! ```
//!
//! Every call goes through the `cache` import, so with a Redis binding named
//! `cache` the counters live in Redis under the binding's prefix and survive a
//! workload restart. The component never sees the Redis address or password.

mod bindings {
    wit_bindgen::generate!({ generate_all });
}

use bindings::exports::wasi::http::incoming_handler::Guest;
use bindings::wasi::http::types::{
    Fields, IncomingRequest, Method, OutgoingBody, OutgoingResponse, ResponseOutparam,
};
use serde_json::json;

struct Component;

/// The bucket a guest opens. Redis has one flat keyspace, so this does not
/// namespace keys; the binding's `prefix` does.
const BUCKET: &str = "counters";
const INDEX_HTML: &str = include_str!("../ui/index.html");

struct Reply {
    status: u16,
    content_type: &'static str,
    body: Vec<u8>,
}

fn json_reply(status: u16, value: serde_json::Value) -> Reply {
    Reply {
        status,
        content_type: "application/json",
        body: value.to_string().into_bytes(),
    }
}

fn error(status: u16, message: impl Into<String>) -> Reply {
    json_reply(status, json!({ "error": message.into() }))
}

/// A counter name: what a person would type, kept to a safe key.
fn valid_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 64
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
}

type Bucket = bindings::cache::Bucket;

fn read(bucket: &Bucket, name: &str) -> Result<u64, String> {
    Ok(bucket
        .get(name)
        .map_err(|e| format!("get {name}: {e:?}"))?
        .and_then(|b| String::from_utf8(b).ok())
        .and_then(|s| s.trim().parse().ok())
        .unwrap_or(0))
}

fn list(bucket: &Bucket) -> Result<serde_json::Value, String> {
    let mut names = Vec::new();
    let mut cursor = None;
    loop {
        let page = bucket
            .list_keys(cursor)
            .map_err(|e| format!("list keys: {e:?}"))?;
        names.extend(page.keys);
        match page.cursor {
            // A cursor of 0 (or none) is the end of the listing.
            Some(next) if next != 0 => cursor = Some(next),
            _ => break,
        }
    }
    names.sort();
    names.dedup();
    let mut counters = Vec::new();
    for name in names {
        counters.push(json!({ "name": name, "value": read(bucket, &name)? }));
    }
    Ok(json!({ "counters": counters }))
}

fn route(method: Method, path: &str) -> Reply {
    let path = path.split('?').next().unwrap_or("/");
    if path == "/" || path == "/index.html" {
        return Reply {
            status: 200,
            content_type: "text/html; charset=utf-8",
            body: INDEX_HTML.as_bytes().to_vec(),
        };
    }
    let bucket = match bindings::cache::open(BUCKET) {
        Ok(b) => b,
        Err(e) => return error(503, format!("open the cache binding: {e:?}")),
    };
    let segments: Vec<&str> = path.trim_matches('/').split('/').collect();
    let result = match (method, segments.as_slice()) {
        (Method::Get, ["api", "counters"]) => list(&bucket).map(|v| json_reply(200, v)),
        (Method::Post, ["api", "counters", name, op @ ("increment" | "decrement")]) => {
            if !valid_name(name) {
                return error(400, "a counter name is letters, digits, '-', '_' or '.', up to 64");
            }
            (|| {
                let current = read(&bucket, name)?;
                let next = if *op == "increment" {
                    current + 1
                } else {
                    current.saturating_sub(1)
                };
                bucket
                    .set(name, next.to_string().as_bytes())
                    .map_err(|e| format!("set {name}: {e:?}"))?;
                Ok(json_reply(200, json!({ "name": name, "value": next })))
            })()
        }
        (Method::Delete, ["api", "counters", name]) => bucket
            .delete(name)
            .map(|()| json_reply(200, json!({ "deleted": name })))
            .map_err(|e| format!("delete {name}: {e:?}")),
        _ => return error(404, format!("no route for {path}")),
    };
    result.unwrap_or_else(|e| error(502, e))
}

impl Guest for Component {
    fn handle(request: IncomingRequest, response_out: ResponseOutparam) {
        let path = request.path_with_query().unwrap_or_else(|| "/".into());
        let reply = route(request.method(), &path);
        let headers = Fields::new();
        let _ = headers.append("content-type", reply.content_type.as_bytes());
        let _ = headers.append("cache-control", b"no-store");
        let response = OutgoingResponse::new(headers);
        let _ = response.set_status_code(reply.status);
        let out_body = response.body().expect("response body");
        ResponseOutparam::set(response_out, Ok(response));
        let stream = out_body.write().expect("body stream");
        for chunk in reply.body.chunks(4096) {
            let _ = stream.blocking_write_and_flush(chunk);
        }
        drop(stream);
        let _ = OutgoingBody::finish(out_body, None);
    }
}

bindings::export!(Component with_types_in bindings);
