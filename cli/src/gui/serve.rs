//! The HTTP layer of sahou gui (design §2.1 serve): embedded assets + file API + SSE.
//! Binds to 127.0.0.1 only. Never parses / validates (it just bridges raw bytes, §1).

use std::io::Write;
use std::sync::mpsc::{Receiver, Sender};
use std::sync::{Arc, Mutex};

use sahou_core::diag::Diag;
use tiny_http::{Header, Method, Request, Response, Server};

use super::files::{etag_of, write_if_match, FileKind, PutError, Targets};

/// Embedded GUI assets (gui/dist). Debug builds reference real files; release builds embed them in the binary (rust-embed default).
#[derive(rust_embed::RustEmbed)]
#[folder = "../gui/dist/"]
struct Assets;

/// SSE subscriber hub. Distributes watch events to all clients (disconnected clients drop out naturally on send failure).
#[derive(Default, Clone)]
pub struct SseHub {
    clients: Arc<Mutex<Vec<Sender<String>>>>,
}

impl SseHub {
    pub fn broadcast(&self, data: &str) {
        let mut clients = self.clients.lock().unwrap();
        clients.retain(|c| c.send(data.to_string()).is_ok());
    }
    fn subscribe(&self) -> Receiver<String> {
        let (tx, rx) = std::sync::mpsc::channel();
        self.clients.lock().unwrap().push(tx);
        rx
    }
}

#[derive(Clone)]
pub struct GuiServer {
    pub addr: std::net::SocketAddr,
    server: Arc<Server>,
    targets: Targets,
    hub: SseHub,
    /// Serializes PUT stat->rename (closes the in-process CAS gap)
    put_lock: Arc<Mutex<()>>,
}

// tiny_http::Server does not implement Debug, so this is a manual impl (the minimum needed for the tests' unwrap_err).
impl std::fmt::Debug for GuiServer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GuiServer")
            .field("addr", &self.addr)
            .finish()
    }
}

pub fn start(targets: Targets, port: u16) -> Result<GuiServer, Vec<Diag>> {
    if !targets.schema.exists() {
        return Err(vec![Diag::new(
            "gui_no_schema",
            targets.schema.display().to_string(),
            "no schema.sahou.yaml (nothing to edit). Launch in the target directory, or specify PATH",
        )]);
    }
    let server = Server::http(("127.0.0.1", port)).map_err(|e| {
        vec![Diag::new(
            "gui_port_in_use",
            format!("127.0.0.1:{port}"),
            format!("cannot bind (port occupied?): {e}. Specify another port with --port (it will not change automatically)"),
        )]
    })?;
    let addr = server
        .server_addr()
        .to_ip()
        .ok_or_else(|| vec![Diag::new("gui_error", "$", "cannot obtain the IP address")])?;
    Ok(GuiServer {
        addr,
        server: Arc::new(server),
        targets,
        hub: SseHub::default(),
        put_lock: Arc::new(Mutex::new(())),
    })
}

fn header(k: &str, v: &str) -> Header {
    Header::from_bytes(k.as_bytes(), v.as_bytes()).expect("a static header is always valid")
}

fn json_response(code: u16, v: &serde_json::Value) -> Response<std::io::Cursor<Vec<u8>>> {
    Response::from_string(v.to_string())
        .with_status_code(code)
        .with_header(header("Content-Type", "application/json; charset=utf-8"))
}

fn text_response(code: u16, s: &str) -> Response<std::io::Cursor<Vec<u8>>> {
    Response::from_string(s).with_status_code(code)
}

fn mime_of(path: &str) -> &'static str {
    match path.rsplit('.').next() {
        Some("html") => "text/html; charset=utf-8",
        Some("js") | Some("mjs") => "text/javascript",
        Some("css") => "text/css",
        Some("wasm") => "application/wasm",
        Some("svg") => "image/svg+xml",
        Some("json") | Some("map") => "application/json",
        Some("ico") => "image/x-icon",
        _ => "application/octet-stream",
    }
}

impl GuiServer {
    pub fn hub(&self) -> SseHub {
        self.hub.clone()
    }

    /// DNS rebinding protection (even with a 127.0.0.1 bind, a malicious web page could reach it via arbitrary DNS).
    /// If the Host header is anything other than the actual bind's 127.0.0.1:<port> / localhost:<port>, return a rejection message.
    /// Additionally, for a PUT (write) that carries an Origin header, require the Origin to match this local
    /// server's origin. Browsers attach an Origin to every non-GET/HEAD request even same-origin, so the mere
    /// presence of Origin must NOT be rejected (that would block the legitimate same-origin autosave); only a
    /// mismatching Origin is rejected (CSRF / cross-origin defense).
    fn reject_cross_origin(&self, req: &Request) -> Option<String> {
        let port = self.addr.port();
        let host = req
            .headers()
            .iter()
            .find(|h| h.field.equiv("Host"))
            .map(|h| h.value.as_str());
        let expected_ip = format!("127.0.0.1:{port}");
        let expected_localhost = format!("localhost:{port}");
        match host {
            Some(h) if h == expected_ip || h == expected_localhost => {}
            _ => {
                return Some(format!(
                    "invalid Host header (rejected by DNS rebinding protection): expected {expected_ip} but got {host:?}"
                ))
            }
        }
        if *req.method() == Method::Put {
            if let Some(origin) = req
                .headers()
                .iter()
                .find(|h| h.field.equiv("Origin"))
                .map(|h| h.value.as_str())
            {
                let ok = origin == format!("http://{expected_ip}")
                    || origin == format!("http://{expected_localhost}");
                if !ok {
                    return Some(format!(
                        "the PUT request's Origin does not match this local server (suspected cross-origin; rejected): got {origin:?}"
                    ));
                }
            }
        }
        None
    }

    /// The request loop (blocks the calling thread). Only SSE is offloaded to a dedicated thread.
    pub fn serve_forever(&self) {
        for req in self.server.incoming_requests() {
            self.handle(req);
        }
    }

    fn handle(&self, req: Request) {
        if let Some(msg) = self.reject_cross_origin(&req) {
            if let Err(e) = req.respond(json_response(403, &serde_json::json!({ "error": msg }))) {
                eprintln!("[warn] gui: failed to send the response: {e}");
            }
            return;
        }
        let url = req.url().to_string();
        let result = match (req.method(), url.as_str()) {
            (Method::Get, "/api/files") => self.get_files(req),
            (Method::Get, "/api/watch") => self.sse(req),
            (Method::Put, p) if p.starts_with("/api/files/") => self.put_file(req, &url),
            (Method::Get, _) => self.asset(req, &url),
            _ => req.respond(text_response(405, "method not allowed")),
        };
        if let Err(e) = result {
            eprintln!("[warn] gui: failed to send the response: {e}");
        }
    }

    fn get_files(&self, req: Request) -> std::io::Result<()> {
        let mut obj = serde_json::Map::new();
        for kind in [FileKind::Schema, FileKind::Layout, FileKind::Endpoints] {
            let path = self.targets.path_of(kind);
            let v = match etag_of(path) {
                Ok(Some(etag)) => match std::fs::read_to_string(path) {
                    Ok(text) => serde_json::json!({ "text": text, "etag": etag }),
                    Err(e) => {
                        return req.respond(json_response(
                            500,
                            &serde_json::json!({ "error": e.to_string() }),
                        ))
                    }
                },
                Ok(None) => serde_json::Value::Null, // layout/endpoints absent = treated as empty (§2.1)
                Err(e) => {
                    return req.respond(json_response(
                        500,
                        &serde_json::json!({ "error": e.to_string() }),
                    ))
                }
            };
            obj.insert(kind.as_str().to_string(), v);
        }
        obj.insert("env".to_string(), serde_json::json!(self.targets.env));
        req.respond(json_response(200, &serde_json::Value::Object(obj)))
    }

    fn put_file(&self, mut req: Request, url: &str) -> std::io::Result<()> {
        let Some(kind) = FileKind::parse(url.trim_start_matches("/api/files/")) else {
            return req.respond(json_response(
                404,
                &serde_json::json!({ "error": "unknown kind (schema|layout|endpoints)" }),
            ));
        };
        let mut body = String::new();
        if let Err(e) = req.as_reader().read_to_string(&mut body) {
            return req.respond(json_response(
                400,
                &serde_json::json!({ "error": e.to_string() }),
            ));
        }
        let if_match = req
            .headers()
            .iter()
            .find(|h| h.field.equiv("If-Match"))
            .map(|h| h.value.as_str().to_string());
        let path = self.targets.path_of(kind).to_path_buf();
        let _guard = self.put_lock.lock().unwrap(); // serialize stat->rename within the process
        match write_if_match(&path, if_match.as_deref(), &body) {
            Ok(etag) => req.respond(json_response(200, &serde_json::json!({ "etag": etag }))),
            Err(PutError::Conflict { current }) => req.respond(json_response(
                409,
                &serde_json::json!({ "etag": current, "error": "etag mismatch (conflict with an external edit)" }),
            )),
            Err(PutError::Io(e)) => {
                req.respond(json_response(500, &serde_json::json!({ "error": e })))
            }
        }
    }

    fn sse(&self, req: Request) -> std::io::Result<()> {
        let rx = self.hub.subscribe();
        // tiny_http's Response(reader) path uses a BufWriter(1024) + chunked Encoder (no flush),
        // so output is buffered until the terminating flush. SSE is an endless stream that never terminates,
        // so not even a single header byte would arrive and it would not work. So we take the raw writer and
        // push the header and each event with a flush each time (the body is delimited by Connection: close).
        // Driven on a dedicated thread (respond/write does not return until disconnect), it ends naturally on disconnect or hub teardown.
        std::thread::spawn(move || {
            let mut w = req.into_writer();
            let push = |w: &mut dyn Write, s: &str| -> std::io::Result<()> {
                w.write_all(s.as_bytes())?;
                w.flush()
            };
            // Status line + SSE headers (close-delimited rather than chunked, to push increments immediately)
            let head = "HTTP/1.1 200 OK\r\n\
                        Content-Type: text/event-stream\r\n\
                        Cache-Control: no-cache\r\n\
                        Connection: close\r\n\r\n";
            if push(&mut *w, head).is_err() || push(&mut *w, "retry: 1000\n\n").is_err() {
                return; // early disconnect
            }
            for data in rx {
                // watch -> SSE. Stream {kind,etag} (including the etag) as-is so self-echoes can be distinguished (§3.4)
                if push(&mut *w, &format!("data: {data}\n\n")).is_err() {
                    break; // client disconnect = write failure
                }
            }
        });
        Ok(())
    }

    fn asset(&self, req: Request, url: &str) -> std::io::Result<()> {
        let rel = url.trim_start_matches('/');
        let rel = if rel.is_empty() { "index.html" } else { rel };
        match Assets::get(rel) {
            Some(f) => {
                let mime = mime_of(rel);
                req.respond(
                    Response::from_data(f.data.into_owned())
                        .with_header(header("Content-Type", mime)),
                )
            }
            None if rel == "index.html" => req.respond(text_response(
                503,
                "GUI assets are not bundled. In gui/, run `npm install && npm run build:core && npm run build`, then rebuild with `cargo build -p sahou`",
            )),
            None => req.respond(text_response(404, "not found")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gui::files::resolve_targets;

    fn boot(dir: &std::path::Path) -> (String, SseHub) {
        std::fs::write(
            dir.join("schema.sahou.yaml"),
            "schema: s\nnodes: {}\nconnections: {}\n",
        )
        .unwrap();
        let server = start(resolve_targets(dir, "dev"), 0).unwrap(); // port 0 = ephemeral
        let url = format!("http://{}", server.addr);
        let hub = server.hub();
        let s2 = server.clone();
        std::thread::spawn(move || s2.serve_forever());
        (url, hub)
    }

    #[test]
    fn files_api_roundtrip_with_cas() {
        let dir = tempfile::tempdir().unwrap();
        let (url, _) = boot(dir.path());
        // GET: schema present / layout and endpoints are null (absent = treated as empty, §2.1)
        let v: serde_json::Value = ureq::get(&format!("{url}/api/files"))
            .call()
            .unwrap()
            .into_json()
            .unwrap();
        assert!(v["schema"]["etag"].is_string());
        assert!(v["layout"].is_null());
        assert_eq!(v["env"], "dev");
        let etag = v["schema"]["etag"].as_str().unwrap().to_string();
        // PUT: correct If-Match -> 200 + new etag + file reflected
        let res = ureq::put(&format!("{url}/api/files/schema"))
            .set("If-Match", &etag)
            .send_string("schema: s2\nnodes: {}\nconnections: {}\n")
            .unwrap();
        let new_etag = res.into_json::<serde_json::Value>().unwrap()["etag"]
            .as_str()
            .unwrap()
            .to_string();
        assert_ne!(new_etag, etag);
        assert!(
            std::fs::read_to_string(dir.path().join("schema.sahou.yaml"))
                .unwrap()
                .contains("s2")
        );
        // PUT: stale etag -> 409 + current etag (compare-and-swap; does not clobber external edits, §2.1)
        match ureq::put(&format!("{url}/api/files/schema"))
            .set("If-Match", &etag)
            .send_string("schema: s3\n")
            .unwrap_err()
        {
            ureq::Error::Status(409, res) => {
                let b: serde_json::Value = res.into_json().unwrap();
                assert_eq!(b["etag"], new_etag.as_str());
            }
            e => panic!("not 409: {e}"),
        }
        // PUT: the first layout is created without If-Match (§2.1)
        let res = ureq::put(&format!("{url}/api/files/layout"))
            .send_string("{\"nodes\":{}}")
            .unwrap();
        assert_eq!(res.status(), 200);
        // an unknown kind is 404
        assert!(matches!(
            ureq::put(&format!("{url}/api/files/nope")).send_string("x"),
            Err(ureq::Error::Status(404, _))
        ));
    }

    #[test]
    fn sse_receives_broadcast() {
        let dir = tempfile::tempdir().unwrap();
        let (url, hub) = boot(dir.path());
        let res = ureq::get(&format!("{url}/api/watch")).call().unwrap();
        assert_eq!(res.header("Content-Type").unwrap(), "text/event-stream");
        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(150));
            hub.broadcast(r#"{"kind":"schema","etag":"x"}"#);
        });
        use std::io::BufRead;
        let mut reader = std::io::BufReader::new(res.into_reader());
        let mut line = String::new();
        loop {
            line.clear();
            reader.read_line(&mut line).unwrap();
            if line.starts_with("data: ") {
                break;
            }
        }
        assert!(line.contains(r#""kind":"schema""#), "{line}");
    }

    #[test]
    fn missing_schema_is_startup_no() {
        let dir = tempfile::tempdir().unwrap();
        let err = start(resolve_targets(dir.path(), "dev"), 0).unwrap_err();
        assert_eq!(err[0].code, "gui_no_schema");
    }

    #[test]
    fn port_in_use_is_no() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("schema.sahou.yaml"),
            "schema: s\nnodes: {}\nconnections: {}\n",
        )
        .unwrap();
        let t = resolve_targets(dir.path(), "dev");
        let first = start(t.clone(), 0).unwrap();
        let err = start(t, first.addr.port()).unwrap_err();
        assert_eq!(err[0].code, "gui_port_in_use"); // rejects instead of auto-incrementing (decision 2)
    }

    #[test]
    fn wrong_host_header_is_rejected() {
        // DNS rebinding: even when bound to 127.0.0.1, a fetch from an arbitrary web page reached via
        // malicious DNS carries the hostname the browser connected to (the attacker's domain) in the Host
        // header. Reject with 403 any Host that does not match the actual bind port.
        let dir = tempfile::tempdir().unwrap();
        let (url, _) = boot(dir.path());
        let err = ureq::get(&format!("{url}/api/files"))
            .set("Host", "evil.example")
            .call()
            .unwrap_err();
        match err {
            ureq::Error::Status(403, _) => {}
            e => panic!("not 403: {e}"),
        }
    }

    #[test]
    fn correct_host_header_is_allowed() {
        // Regression check for the happy path: a Host of 127.0.0.1:<actual port> is allowed.
        let dir = tempfile::tempdir().unwrap();
        let (url, _) = boot(dir.path());
        let addr = url.trim_start_matches("http://");
        let res = ureq::get(&format!("{url}/api/files"))
            .set("Host", addr)
            .call()
            .unwrap();
        assert_eq!(res.status(), 200);
    }

    #[test]
    fn put_with_mismatched_origin_is_rejected() {
        // A PUT whose Origin does not match this local server is suspected cross-origin and rejected with 403.
        let dir = tempfile::tempdir().unwrap();
        let (url, _) = boot(dir.path());
        let err = ureq::put(&format!("{url}/api/files/layout"))
            .set("Origin", "http://evil.example")
            .send_string("{\"nodes\":{}}")
            .unwrap_err();
        match err {
            ureq::Error::Status(403, _) => {}
            e => panic!("not 403: {e}"),
        }
    }

    #[test]
    fn put_with_matching_origin_is_allowed() {
        // Browsers attach an Origin to every same-origin non-GET/HEAD request (including PUT), so a matching
        // Origin MUST be allowed — otherwise the legitimate same-origin autosave is blocked (the autosave bug).
        let dir = tempfile::tempdir().unwrap();
        let (url, _) = boot(dir.path());
        let res = ureq::put(&format!("{url}/api/files/layout"))
            .set("Origin", &url) // url == http://127.0.0.1:<actual port> = this server's origin
            .send_string("{\"nodes\":{}}")
            .unwrap();
        assert_eq!(res.status(), 200);
    }

    #[test]
    fn asset_fallback_explains_build_when_missing() {
        let dir = tempfile::tempdir().unwrap();
        let (url, _) = boot(dir.path());
        match ureq::get(&url).call() {
            // assets not built -> 503 + build instructions (do not silently show a blank page, §7)
            Err(ureq::Error::Status(503, res)) => {
                assert!(res.into_string().unwrap().contains("npm run build"))
            }
            // in an environment where assets are built, index.html is returned (both are valid)
            Ok(res) => assert_eq!(
                res.header("Content-Type").unwrap(),
                "text/html; charset=utf-8"
            ),
            Err(e) => panic!("{e}"),
        }
    }
}
