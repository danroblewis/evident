//! Protocol-level integration tests: spawn the built `evident-lsp` binary,
//! perform the LSP `initialize` handshake over stdio JSON-RPC, then drive real
//! requests against actual `compiler2/*.ev` content and assert the responses.
//!
//! These are headless equivalents of "open the file in an editor and click":
//! they exercise the exact JSON-RPC the server speaks, so a green run proves
//! the wire protocol works end-to-end (framing, UTF-16 positions, capability
//! routing) — not just the library internals.

use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Read, Write};
use std::path::PathBuf;
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

/// A spawned server with framed-stdio plumbing.
struct Lsp {
    child: Child,
    stdin: Option<ChildStdin>,
    stdout: BufReader<ChildStdout>,
    next_id: i64,
}

fn repo_root() -> PathBuf {
    // tests run with CWD = crate dir (tools/evident-lsp). Walk up to repo root.
    let mut d = std::env::current_dir().unwrap();
    loop {
        if d.join("CLAUDE.md").exists() && d.join("compiler2").exists() {
            return d;
        }
        if !d.pop() {
            panic!("could not find repo root");
        }
    }
}

fn server_bin() -> PathBuf {
    // CARGO_BIN_EXE_<name> points at the freshly built test binary.
    PathBuf::from(env!("CARGO_BIN_EXE_evident-lsp"))
}

impl Lsp {
    fn spawn() -> Lsp {
        let root = repo_root();
        let mut child = Command::new(server_bin())
            .current_dir(&root)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("spawn evident-lsp");
        let stdin = child.stdin.take().unwrap();
        let stdout = BufReader::new(child.stdout.take().unwrap());
        Lsp {
            child,
            stdin: Some(stdin),
            stdout,
            next_id: 1,
        }
    }

    fn send(&mut self, msg: &Value) {
        let body = serde_json::to_string(msg).unwrap();
        let stdin = self.stdin.as_mut().expect("stdin open");
        write!(stdin, "Content-Length: {}\r\n\r\n{}", body.len(), body).unwrap();
        stdin.flush().unwrap();
    }

    fn request(&mut self, method: &str, params: Value) -> Value {
        let id = self.next_id;
        self.next_id += 1;
        self.send(&json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params,
        }));
        // read until we get a response with this id (skipping notifications)
        loop {
            let v = self.read_message();
            if v.get("id").and_then(|i| i.as_i64()) == Some(id) {
                return v;
            }
            // else it's a notification (diagnostics, log) — keep reading
        }
    }

    fn notify(&mut self, method: &str, params: Value) {
        self.send(&json!({
            "jsonrpc": "2.0",
            "method": method,
            "params": params,
        }));
    }

    /// Read one framed message; loops past anything that isn't ours.
    fn read_message(&mut self) -> Value {
        // read headers
        let mut content_len = 0usize;
        loop {
            let mut line = String::new();
            let n = self.stdout.read_line(&mut line).unwrap();
            if n == 0 {
                panic!("server closed stdout unexpectedly");
            }
            let trimmed = line.trim_end();
            if trimmed.is_empty() {
                break;
            }
            if let Some(v) = trimmed.strip_prefix("Content-Length:") {
                content_len = v.trim().parse().unwrap();
            }
        }
        let mut buf = vec![0u8; content_len];
        self.stdout.read_exact(&mut buf).unwrap();
        serde_json::from_slice(&buf).unwrap()
    }

    /// Read messages until one matches a predicate (e.g. a specific
    /// notification). Used to capture publishDiagnostics.
    fn read_until<F: Fn(&Value) -> bool>(&mut self, pred: F) -> Value {
        for _ in 0..50 {
            let v = self.read_message();
            if pred(&v) {
                return v;
            }
        }
        panic!("did not receive expected message");
    }

    fn initialize(&mut self) -> Value {
        let root = repo_root();
        let root_uri = url_for(&root);
        let resp = self.request(
            "initialize",
            json!({
                "processId": null,
                "rootUri": root_uri,
                "capabilities": {},
                "workspaceFolders": [{
                    "uri": root_uri,
                    "name": "evident"
                }]
            }),
        );
        self.notify("initialized", json!({}));
        resp
    }

    fn open(&mut self, path: &PathBuf) -> String {
        let text = std::fs::read_to_string(path).unwrap();
        let uri = url_for(path);
        self.notify(
            "textDocument/didOpen",
            json!({
                "textDocument": {
                    "uri": uri,
                    "languageId": "evident",
                    "version": 1,
                    "text": text,
                }
            }),
        );
        uri
    }

    fn shutdown(&mut self) {
        // `shutdown` takes no params (tower-lsp rejects `params: null`).
        let id = self.next_id;
        self.next_id += 1;
        self.send(&json!({ "jsonrpc": "2.0", "id": id, "method": "shutdown" }));
        loop {
            let v = self.read_message();
            if v.get("id").and_then(|i| i.as_i64()) == Some(id) {
                break;
            }
        }
        self.send(&json!({ "jsonrpc": "2.0", "method": "exit", "params": null }));
        // Close stdin so the server's stdin reader hits EOF and the serve loop
        // terminates; then reap.
        drop(self.stdin.take());
        let _ = self.child.wait();
    }
}

fn url_for(p: &std::path::Path) -> String {
    tower_lsp_url(p)
}

// minimal file:// url builder matching tower-lsp's Url::from_file_path
fn tower_lsp_url(p: &std::path::Path) -> String {
    let s = p.to_string_lossy();
    let mut out = String::from("file://");
    for c in s.chars() {
        match c {
            '/' => out.push('/'),
            'a'..='z' | 'A'..='Z' | '0'..='9' | '-' | '_' | '.' | '~' => out.push(c),
            _ => {
                let mut buf = [0u8; 4];
                for b in c.encode_utf8(&mut buf).bytes() {
                    out.push_str(&format!("%{:02X}", b));
                }
            }
        }
    }
    out
}

/// Find the 0-based (line, utf16 char) of the first occurrence of `needle` in
/// `text`, offset into it by `within` chars. Used to point the cursor at a
/// known token. Returns LSP coords.
fn pos_of(text: &str, needle: &str, within: usize) -> (u32, u32) {
    let byte = text.find(needle).expect("needle present");
    let mut line = 0u32;
    let mut col_u16 = 0u32;
    let mut b = 0usize;
    for c in text.chars() {
        if b >= byte {
            break;
        }
        if c == '\n' {
            line += 1;
            col_u16 = 0;
        } else {
            col_u16 += c.len_utf16() as u32;
        }
        b += c.len_utf8();
    }
    (line, col_u16 + within as u32)
}

fn compiler2(name: &str) -> PathBuf {
    repo_root().join("compiler2").join(name)
}

// ── tests ────────────────────────────────────────────────────────────────

#[test]
fn initialize_advertises_capabilities() {
    let mut lsp = Lsp::spawn();
    let resp = lsp.initialize();
    let caps = &resp["result"]["capabilities"];
    assert!(caps["definitionProvider"].as_bool() == Some(true));
    assert!(caps["referencesProvider"].as_bool() == Some(true));
    assert!(caps["documentSymbolProvider"].as_bool() == Some(true));
    assert!(caps["renameProvider"]["prepareProvider"].as_bool() == Some(true));
    assert!(caps["hoverProvider"].as_bool() == Some(true));
    assert!(caps["completionProvider"].is_object());
    // incremental sync == 2
    assert_eq!(caps["textDocumentSync"].as_i64(), Some(2));
    assert_eq!(resp["result"]["serverInfo"]["name"].as_str(), Some("evident-lsp"));
    lsp.shutdown();
}

#[test]
fn definition_of_a_type() {
    let mut lsp = Lsp::spawn();
    lsp.initialize();
    let file = compiler2("driver_setvar.ev");
    let text = std::fs::read_to_string(&file).unwrap();
    let uri = lsp.open(&file);
    // `QsetName` is declared as a `type` in this file and used as Seq(QsetName).
    // Point at the USE site `unpacked_names ∈ Seq(QsetName)` (line 113) — the
    // first `Seq(QsetName)` literal is inside a comment, which has no token.
    let (line, ch) = pos_of(&text, "unpacked_names ∈ Seq(QsetName)", 21); // on "Q" of QsetName
    let resp = lsp.request(
        "textDocument/definition",
        json!({
            "textDocument": { "uri": uri },
            "position": { "line": line, "character": ch },
        }),
    );
    let arr = resp["result"].as_array().expect("definition array");
    assert!(!arr.is_empty(), "expected at least one definition, got {:?}", resp);
    // the def should be a `type QsetName` line
    let def_line = arr[0]["range"]["start"]["line"].as_u64().unwrap() as usize;
    let def_text = text.lines().nth(def_line).unwrap_or("");
    assert!(
        def_text.contains("QsetName"),
        "def line {} = {:?}",
        def_line,
        def_text
    );
    lsp.shutdown();
}

#[test]
fn references_includes_declaration() {
    let mut lsp = Lsp::spawn();
    lsp.initialize();
    let file = compiler2("driver_setvar.ev");
    let text = std::fs::read_to_string(&file).unwrap();
    let uri = lsp.open(&file);
    let (line, ch) = pos_of(&text, "set_registry_count ∈ Int", 3);
    let resp = lsp.request(
        "textDocument/references",
        json!({
            "textDocument": { "uri": uri },
            "position": { "line": line, "character": ch },
            "context": { "includeDeclaration": true },
        }),
    );
    let arr = resp["result"].as_array().expect("references array");
    // set_registry_count is referenced many times in this file
    assert!(arr.len() >= 3, "expected ≥3 refs, got {}", arr.len());
    lsp.shutdown();
}

#[test]
fn document_symbol_outline() {
    let mut lsp = Lsp::spawn();
    lsp.initialize();
    let file = compiler2("driver_setvar.ev");
    let uri = lsp.open(&file);
    let resp = lsp.request(
        "textDocument/documentSymbol",
        json!({ "textDocument": { "uri": uri } }),
    );
    let arr = resp["result"].as_array().expect("symbol array");
    let names: Vec<&str> = arr.iter().filter_map(|s| s["name"].as_str()).collect();
    assert!(names.contains(&"DriverSetVar"), "names = {:?}", names);
    assert!(names.contains(&"QsetName"), "names = {:?}", names);
    // DriverSetVar should have member children
    let dsv = arr.iter().find(|s| s["name"] == "DriverSetVar").unwrap();
    let children = dsv["children"].as_array().expect("children");
    assert!(!children.is_empty(), "DriverSetVar should have members");
    lsp.shutdown();
}

#[test]
fn rename_succeeds_on_fresh_name() {
    let mut lsp = Lsp::spawn();
    lsp.initialize();
    let file = compiler2("driver_setvar.ev");
    let text = std::fs::read_to_string(&file).unwrap();
    let uri = lsp.open(&file);
    let (line, ch) = pos_of(&text, "active_set_name ∈ String", 3);
    let resp = lsp.request(
        "textDocument/rename",
        json!({
            "textDocument": { "uri": uri },
            "position": { "line": line, "character": ch },
            "newName": "active_set_name_zzqq_unique",
        }),
    );
    let changes = resp["result"]["changes"]
        .as_object()
        .expect("rename changes");
    assert!(!changes.is_empty(), "expected edits, got {:?}", resp);
    // and the edits should target multiple occurrences in this file
    let edits = changes.values().next().unwrap().as_array().unwrap();
    assert!(edits.len() >= 2, "expected ≥2 edits");
    lsp.shutdown();
}

#[test]
fn rename_refused_on_collision() {
    let mut lsp = Lsp::spawn();
    lsp.initialize();
    let file = compiler2("driver_setvar.ev");
    let text = std::fs::read_to_string(&file).unwrap();
    let uri = lsp.open(&file);
    // rename `active_set_name` → `vars` (an existing decl in the same file).
    let (line, ch) = pos_of(&text, "active_set_name ∈ String", 3);
    let resp = lsp.request(
        "textDocument/rename",
        json!({
            "textDocument": { "uri": uri },
            "position": { "line": line, "character": ch },
            "newName": "vars",
        }),
    );
    // MUST be an error, not a silent edit (the merge trap).
    assert!(
        resp.get("error").is_some(),
        "expected error on colliding rename, got {:?}",
        resp
    );
    assert!(resp.get("result").map(|r| r.is_null()).unwrap_or(true));
    let msg = resp["error"]["message"].as_str().unwrap_or("");
    assert!(msg.contains("MERGE"), "error message = {:?}", msg);
    lsp.shutdown();
}

#[test]
fn publish_diagnostics_on_seq_membership() {
    let mut lsp = Lsp::spawn();
    lsp.initialize();
    // A synthetic doc with the Seq-membership footgun, opened as a virtual uri.
    let uri = "file:///tmp/evident_lsp_diag_test.ev";
    let src = "claim main\n    xs ∈ Seq(Int) = ⟨1, 2, 3⟩\n    target ∈ Int = 99\n    target ∈ xs\n";
    lsp.notify(
        "textDocument/didOpen",
        json!({
            "textDocument": {
                "uri": uri,
                "languageId": "evident",
                "version": 1,
                "text": src,
            }
        }),
    );
    let diag = lsp.read_until(|v| {
        v.get("method").and_then(|m| m.as_str()) == Some("textDocument/publishDiagnostics")
            && v["params"]["uri"].as_str() == Some(uri)
    });
    let arr = diag["params"]["diagnostics"].as_array().unwrap();
    assert!(
        arr.iter()
            .any(|d| d["code"].as_str() == Some("seq-membership")),
        "expected a seq-membership diagnostic, got {:?}",
        arr
    );
    // and the range should be on line 3 (0-based) where `target ∈ xs` is
    let d = arr
        .iter()
        .find(|d| d["code"].as_str() == Some("seq-membership"))
        .unwrap();
    assert_eq!(d["range"]["start"]["line"].as_u64(), Some(3));
    lsp.shutdown();
}

#[test]
fn hover_shows_kind_and_signature() {
    let mut lsp = Lsp::spawn();
    lsp.initialize();
    let file = compiler2("driver_setvar.ev");
    let text = std::fs::read_to_string(&file).unwrap();
    let uri = lsp.open(&file);
    let (line, ch) = pos_of(&text, "fsm DriverSetVar", 5); // inside DriverSetVar
    let resp = lsp.request(
        "textDocument/hover",
        json!({
            "textDocument": { "uri": uri },
            "position": { "line": line, "character": ch },
        }),
    );
    let val = resp["result"]["contents"]["value"].as_str().unwrap_or("");
    assert!(val.contains("DriverSetVar"), "hover = {:?}", val);
    lsp.shutdown();
}

#[test]
fn completion_returns_keywords_and_symbols() {
    let mut lsp = Lsp::spawn();
    lsp.initialize();
    let uri = "file:///tmp/evident_lsp_complete_test.ev";
    let src = "claim main\n    x ∈ Int\n    Driver";
    lsp.notify(
        "textDocument/didOpen",
        json!({
            "textDocument": {
                "uri": uri,
                "languageId": "evident",
                "version": 1,
                "text": src,
            }
        }),
    );
    // cursor right after "Driver" on line 2 (0-based), char 10
    let resp = lsp.request(
        "textDocument/completion",
        json!({
            "textDocument": { "uri": uri },
            "position": { "line": 2, "character": 10 },
        }),
    );
    let items = resp["result"].as_array().expect("completion array");
    let labels: Vec<&str> = items.iter().filter_map(|i| i["label"].as_str()).collect();
    assert!(
        labels.iter().all(|l| l.starts_with("Driver")),
        "all completions should match prefix 'Driver', got {:?}",
        labels
    );
    assert!(!labels.is_empty(), "expected Driver* completions");
    lsp.shutdown();
}
