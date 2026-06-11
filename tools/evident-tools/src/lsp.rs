//! evident-lsp — a minimal Language Server for Evident, std-only.
//!
//! Reuses the `evident_tools` engine (lexer + index + rename + lint glue).
//! Supports: initialize, textDocument/{definition, references, documentSymbol,
//! hover, rename, prepareRename}, and publishes basic diagnostics (the
//! Seq-membership lint, run via scripts/lint-seq-membership.sh when present,
//! else a built-in token check).
//!
//! Transport: LSP framing over stdio (`Content-Length` headers + JSON-RPC).
//! Positions are LSP (0-based line, 0-based UTF-16 column); we convert to/from
//! our 1-based char columns.

#[path = "json.rs"]
mod json;

use evident_tools::index::{self, RefKind};
use evident_tools::lexer::{self, Tok};
use evident_tools::rename;
use json::J;
use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

fn main() {
    let mut server = Server::new();
    server.run();
}

struct Server {
    root: PathBuf,
    /// open-buffer overrides: uri → text.
    open: BTreeMap<String, String>,
}

impl Server {
    fn new() -> Self {
        Server {
            root: find_root(),
            open: BTreeMap::new(),
        }
    }

    fn run(&mut self) {
        let mut stdin = std::io::stdin();
        loop {
            let msg = match read_message(&mut stdin) {
                Some(m) => m,
                None => break,
            };
            let v = match json::parse(&msg) {
                Some(v) => v,
                None => continue,
            };
            let method = v.get("method").and_then(|m| m.as_str()).unwrap_or("");
            let id = v.get("id").cloned();
            match method {
                "initialize" => self.reply(id, self.capabilities()),
                "initialized" => {}
                "shutdown" => self.reply(id, J::Null),
                "exit" => break,
                "textDocument/didOpen" => self.did_open(&v),
                "textDocument/didChange" => self.did_change(&v),
                "textDocument/didClose" => self.did_close(&v),
                "textDocument/definition" => {
                    let r = self.definition(&v);
                    self.reply(id, r);
                }
                "textDocument/references" => {
                    let r = self.references(&v);
                    self.reply(id, r);
                }
                "textDocument/documentSymbol" => {
                    let r = self.document_symbol(&v);
                    self.reply(id, r);
                }
                "textDocument/hover" => {
                    let r = self.hover(&v);
                    self.reply(id, r);
                }
                "textDocument/prepareRename" => {
                    let r = self.prepare_rename(&v);
                    self.reply(id, r);
                }
                "textDocument/rename" => {
                    let r = self.rename(&v);
                    self.reply(id, r);
                }
                _ => {
                    if id.is_some() {
                        // method not found — reply null result to keep client happy
                        self.reply(id, J::Null);
                    }
                }
            }
        }
    }

    fn capabilities(&self) -> J {
        let sync = J::obj()
            .set("openClose", J::Bool(true))
            .set("change", J::Num(1.0)); // full sync
        let caps = J::obj()
            .set("textDocumentSync", sync)
            .set("definitionProvider", J::Bool(true))
            .set("referencesProvider", J::Bool(true))
            .set("documentSymbolProvider", J::Bool(true))
            .set("hoverProvider", J::Bool(true))
            .set("renameProvider", J::obj().set("prepareProvider", J::Bool(true)));
        J::obj().set("capabilities", caps).set(
            "serverInfo",
            J::obj()
                .set("name", J::Str("evident-lsp".into()))
                .set("version", J::Str("0.1.0".into())),
        )
    }

    // ── message plumbing ────────────────────────────────────────────────

    fn reply(&self, id: Option<J>, result: J) {
        let msg = J::obj()
            .set("jsonrpc", J::Str("2.0".into()))
            .set("id", id.unwrap_or(J::Null))
            .set("result", result);
        write_message(&msg.to_string());
    }

    fn notify(&self, method: &str, params: J) {
        let msg = J::obj()
            .set("jsonrpc", J::Str("2.0".into()))
            .set("method", J::Str(method.into()))
            .set("params", params);
        write_message(&msg.to_string());
    }

    // ── doc lifecycle ───────────────────────────────────────────────────

    fn did_open(&mut self, v: &J) {
        if let Some(td) = v.get("params").and_then(|p| p.get("textDocument")) {
            if let (Some(uri), Some(text)) = (
                td.get("uri").and_then(|u| u.as_str()),
                td.get("text").and_then(|t| t.as_str()),
            ) {
                self.open.insert(uri.to_string(), text.to_string());
                self.publish_diag(uri);
            }
        }
    }
    fn did_change(&mut self, v: &J) {
        let params = match v.get("params") {
            Some(p) => p,
            None => return,
        };
        let uri = params
            .get("textDocument")
            .and_then(|t| t.get("uri"))
            .and_then(|u| u.as_str())
            .map(str::to_string);
        // full sync: last change is the full text
        if let (Some(uri), Some(changes)) =
            (uri, params.get("contentChanges").and_then(|c| c.as_arr()))
        {
            if let Some(last) = changes.last() {
                if let Some(text) = last.get("text").and_then(|t| t.as_str()) {
                    self.open.insert(uri.clone(), text.to_string());
                    self.publish_diag(&uri);
                }
            }
        }
    }
    fn did_close(&mut self, v: &J) {
        if let Some(uri) = v
            .get("params")
            .and_then(|p| p.get("textDocument"))
            .and_then(|t| t.get("uri"))
            .and_then(|u| u.as_str())
        {
            self.open.remove(uri);
        }
    }

    // ── engine helpers ──────────────────────────────────────────────────

    /// All .ev files in the tree, with open-buffer overrides applied.
    fn all_files(&self) -> Vec<(PathBuf, String)> {
        let mut paths = Vec::new();
        for d in ["compiler2", "stdlib", "compiler", "tests"] {
            let p = self.root.join(d);
            if p.exists() {
                walk(&p, &mut paths);
            }
        }
        paths.sort();
        paths.dedup();
        let mut files: Vec<(PathBuf, String)> = paths
            .iter()
            .filter_map(|p| std::fs::read_to_string(p).ok().map(|s| (p.clone(), s)))
            .collect();
        // overlay open buffers
        for (uri, text) in &self.open {
            if let Some(path) = uri_to_path(uri) {
                if let Some(slot) = files.iter_mut().find(|(p, _)| *p == path) {
                    slot.1 = text.clone();
                } else {
                    files.push((path, text.clone()));
                }
            }
        }
        files
    }

    fn doc_text(&self, uri: &str) -> Option<String> {
        if let Some(t) = self.open.get(uri) {
            return Some(t.clone());
        }
        uri_to_path(uri).and_then(|p| std::fs::read_to_string(p).ok())
    }

    /// Resolve the identifier base name at an LSP position in `uri`.
    fn name_at(&self, uri: &str, line0: usize, col0_utf16: usize) -> Option<String> {
        let text = self.doc_text(uri)?;
        let toks = lexer::lex(&text);
        // convert LSP (line0, utf16col) to our (1-based line, 1-based char col)
        let target_line = line0 + 1;
        let target_col = utf16_to_char_col(&text, line0, col0_utf16) + 1;
        for t in &toks {
            if let Tok::Ident(w) = &t.tok {
                let len_chars = w.chars().count();
                if t.line == target_line
                    && target_col >= t.col
                    && target_col < t.col + len_chars
                {
                    let base = w
                        .strip_prefix('_')
                        .filter(|r| !r.is_empty())
                        .unwrap_or(w);
                    return Some(base.to_string());
                }
            }
        }
        None
    }

    // ── requests ────────────────────────────────────────────────────────

    fn definition(&self, v: &J) -> J {
        let (uri, l, c) = match pos_of(v) {
            Some(x) => x,
            None => return J::Null,
        };
        let name = match self.name_at(&uri, l, c) {
            Some(n) => n,
            None => return J::Null,
        };
        let files = self.all_files();
        let idx = index::build_index(&files);
        let mut locs = Vec::new();
        for o in &idx.occurrences {
            if o.name == name && o.kind.is_decl() {
                locs.push(self.location(&o.file, o.line, o.col, name.chars().count()));
            }
        }
        if locs.is_empty() {
            J::Null
        } else {
            J::Arr(locs)
        }
    }

    fn references(&self, v: &J) -> J {
        let (uri, l, c) = match pos_of(v) {
            Some(x) => x,
            None => return J::Arr(vec![]),
        };
        let include_decl = v
            .get("params")
            .and_then(|p| p.get("context"))
            .and_then(|c| c.get("includeDeclaration"))
            .map(|b| matches!(b, J::Bool(true)))
            .unwrap_or(true);
        let name = match self.name_at(&uri, l, c) {
            Some(n) => n,
            None => return J::Arr(vec![]),
        };
        let files = self.all_files();
        let idx = index::build_index(&files);
        let mut locs = Vec::new();
        for o in &idx.occurrences {
            if o.name == name {
                if !include_decl && o.kind.is_decl() {
                    continue;
                }
                // the on-disk col is the base col; for a dual the underscore
                // shifts the displayed name but our col points at the token
                // start (the underscore), so widen length by 1 for duals.
                let len = name.chars().count() + if o.is_dual { 1 } else { 0 };
                locs.push(self.location(&o.file, o.line, o.col, len));
            }
        }
        J::Arr(locs)
    }

    fn document_symbol(&self, v: &J) -> J {
        let uri = match v
            .get("params")
            .and_then(|p| p.get("textDocument"))
            .and_then(|t| t.get("uri"))
            .and_then(|u| u.as_str())
        {
            Some(u) => u.to_string(),
            None => return J::Arr(vec![]),
        };
        let path = match uri_to_path(&uri) {
            Some(p) => p,
            None => return J::Arr(vec![]),
        };
        let text = self.doc_text(&uri).unwrap_or_default();
        let mut idx = index::Index::default();
        index::index_file(&path, &text, &mut idx);

        // SymbolKind: Class=5, Field=8, Enum=10, EnumMember=22, Struct=23.
        let mut syms = Vec::new();
        let mut decls = idx.decls.clone();
        decls.sort_by_key(|d| (d.line, d.col));
        for d in &decls {
            let kind = match d.kind {
                index::DeclKind::Enum => 10,
                index::DeclKind::EnumVariant => continue, // nested under enum below
                index::DeclKind::Type | index::DeclKind::Schema => 23,
                _ => 5,
            };
            // children: member decls in this scope
            let mut children = Vec::new();
            for o in &idx.occurrences {
                if o.scope == d.name
                    && matches!(o.kind, RefKind::MemberDecl | RefKind::HeaderSlot)
                {
                    children.push(symbol_node(
                        &o.name,
                        8,
                        o.line,
                        o.col,
                        o.name.chars().count(),
                        vec![],
                    ));
                }
            }
            // enum variants as members
            if d.kind == index::DeclKind::Enum {
                for o in &idx.occurrences {
                    if o.scope == d.name && o.kind == RefKind::VariantDecl {
                        children.push(symbol_node(
                            &o.name,
                            22,
                            o.line,
                            o.col,
                            o.name.chars().count(),
                            vec![],
                        ));
                    }
                }
            }
            syms.push(symbol_node(
                &d.name,
                kind,
                d.line,
                d.col,
                d.name.chars().count(),
                children,
            ));
        }
        J::Arr(syms)
    }

    fn hover(&self, v: &J) -> J {
        let (uri, l, c) = match pos_of(v) {
            Some(x) => x,
            None => return J::Null,
        };
        let name = match self.name_at(&uri, l, c) {
            Some(n) => n,
            None => return J::Null,
        };
        let files = self.all_files();
        let idx = index::build_index(&files);
        // Prefer a schema decl, else a member decl.
        let mut lines = Vec::new();
        for d in &idx.decls {
            if d.name == name {
                let hdr = if d.header_slots.is_empty() {
                    String::new()
                } else {
                    format!("({})", d.header_slots.join(", "))
                };
                lines.push(format!("{} {}{}", d.kind.label(), d.name, hdr));
            }
        }
        if lines.is_empty() {
            // member variable
            let nrefs = idx.occurrences.iter().filter(|o| o.name == name).count();
            lines.push(format!("variable `{}` — {} occurrence(s)", name, nrefs));
        }
        let md = format!("```evident\n{}\n```", dedup(lines).join("\n"));
        J::obj().set(
            "contents",
            J::obj()
                .set("kind", J::Str("markdown".into()))
                .set("value", J::Str(md)),
        )
    }

    fn prepare_rename(&self, v: &J) -> J {
        let (uri, l, c) = match pos_of(v) {
            Some(x) => x,
            None => return J::Null,
        };
        match self.name_at(&uri, l, c) {
            Some(n) => J::obj()
                .set("placeholder", J::Str(n))
                .set("range", self.token_range_at(&uri, l, c)),
            None => J::Null,
        }
    }

    fn rename(&self, v: &J) -> J {
        let (uri, l, c) = match pos_of(v) {
            Some(x) => x,
            None => return J::Null,
        };
        let new_name = match v
            .get("params")
            .and_then(|p| p.get("newName"))
            .and_then(|n| n.as_str())
        {
            Some(n) => n.to_string(),
            None => return J::Null,
        };
        let old = match self.name_at(&uri, l, c) {
            Some(n) => n,
            None => return J::Null,
        };
        let new_base = new_name
            .strip_prefix('_')
            .filter(|r| !r.is_empty())
            .unwrap_or(&new_name)
            .to_string();
        if !rename::valid_ident(&new_base) || old == new_base {
            return J::Null;
        }
        let files = self.all_files();
        // collision guard — surface as a window/showMessage but still allow
        // the edit? LSP rename has no "refuse with reason" beyond an error;
        // we conservatively REFUSE by returning null + a showMessage, matching
        // the CLI's safety posture (the merge trap is dangerous).
        if rename::count_base(&files, &new_base) > 0 {
            self.notify(
                "window/showMessage",
                J::obj().set("type", J::Num(1.0)).set(
                    "message",
                    J::Str(format!(
                        "evident-lsp: refusing rename '{old}'→'{new_base}' — target already exists (would MERGE symbols). Use the `evt rename --force` CLI if intended."
                    )),
                ),
            );
            return J::Null;
        }
        let edits = rename::compute(&files, &old, &new_base);
        // Build a WorkspaceEdit { changes: { uri: [TextEdit] } }.
        let mut changes = BTreeMap::new();
        for fe in &edits {
            let text = std::fs::read_to_string(&fe.path).unwrap_or_default();
            let text = self
                .open
                .get(&path_to_uri(&fe.path))
                .cloned()
                .unwrap_or(text);
            let mut tes = Vec::new();
            for e in &fe.edits {
                let (sl, sc) = byte_to_lsp(&text, e.byte_start);
                let (el, ec) = byte_to_lsp(&text, e.byte_end);
                tes.push(
                    J::obj()
                        .set(
                            "range",
                            J::obj()
                                .set("start", lsp_pos(sl, sc))
                                .set("end", lsp_pos(el, ec)),
                        )
                        .set("newText", J::Str(e.new_text.clone())),
                );
            }
            changes.insert(path_to_uri(&fe.path), J::Arr(tes));
        }
        J::obj().set("changes", J::Obj(changes))
    }

    // ── diagnostics ─────────────────────────────────────────────────────

    fn publish_diag(&self, uri: &str) {
        let text = match self.doc_text(uri) {
            Some(t) => t,
            None => return,
        };
        let mut diags = Vec::new();
        // Built-in Seq-membership lint: flag `x ∈ xs` where xs is a Seq —
        // we can't know the type without the oracle, so we flag the SHAPE
        // `IDENT ∈ IDENT` at expression position only as an informational
        // hint when the RHS looks like a seq (lowercase plural-ish). To avoid
        // false positives we keep it conservative: flag `∈` membership whose
        // RHS is a bare lowercase identifier (not a Type/Capitalized, not a
        // Set(...)/Seq(...) constructor) AND not at a top-level decl line.
        for (lnum, line) in text.lines().enumerate() {
            // find ` ∈ ` occurrences not part of a decl/quantifier
            if let Some(bidx) = line.find('∈') {
                let after = line[bidx + '∈'.len_utf8()..].trim_start();
                // first token after ∈
                let rhs: String = after
                    .chars()
                    .take_while(|c| *c == '_' || c.is_ascii_alphanumeric())
                    .collect();
                let is_lower_ident = rhs
                    .chars()
                    .next()
                    .map(|c| c == '_' || c.is_ascii_lowercase())
                    .unwrap_or(false);
                // left side single bare ident (heuristic membership-in-expr)
                let before = line[..bidx].trim_end();
                let lhs_last: String = before
                    .chars()
                    .rev()
                    .take_while(|c| *c == '_' || c.is_ascii_alphanumeric())
                    .collect::<String>()
                    .chars()
                    .rev()
                    .collect();
                let in_quantifier = before.contains('∀') || before.contains('∃');
                let is_decl_line = !before.contains('=')
                    && before
                        .trim_start()
                        .chars()
                        .next()
                        .map(|c| c == '_' || c.is_ascii_alphabetic())
                        .unwrap_or(false)
                    && !before.contains('(');
                if is_lower_ident
                    && !in_quantifier
                    && !is_decl_line
                    && !lhs_last.is_empty()
                    && (before.contains('?') || before.contains('('))
                {
                    let col = line[..bidx].chars().count();
                    diags.push(diag(
                        lnum,
                        col.saturating_sub(lhs_last.chars().count() + 1),
                        col + 1,
                        2, // Warning
                        format!(
                            "possible Seq membership `{} ∈ {}` — SILENTLY DROPPED by the oracle; use `∃ i ∈ {{0..#{}-1}} : {}[i] = {}`",
                            lhs_last, rhs, rhs, rhs, lhs_last
                        ),
                    ));
                }
            }
        }
        self.notify(
            "textDocument/publishDiagnostics",
            J::obj()
                .set("uri", J::Str(uri.to_string()))
                .set("diagnostics", J::Arr(diags)),
        );
    }

    // ── location/range builders ─────────────────────────────────────────

    fn location(&self, path: &Path, line1: usize, col1: usize, len_chars: usize) -> J {
        let text = std::fs::read_to_string(path).unwrap_or_default();
        let (sl, sc) = char_to_lsp(&text, line1, col1);
        J::obj()
            .set("uri", J::Str(path_to_uri(path)))
            .set(
                "range",
                J::obj()
                    .set("start", lsp_pos(sl, sc))
                    .set("end", lsp_pos(sl, sc + len_chars)),
            )
    }

    fn token_range_at(&self, uri: &str, line0: usize, col0: usize) -> J {
        let text = self.doc_text(uri).unwrap_or_default();
        let toks = lexer::lex(&text);
        let target_line = line0 + 1;
        let target_col = utf16_to_char_col(&text, line0, col0) + 1;
        for t in &toks {
            if let Tok::Ident(w) = &t.tok {
                let len = w.chars().count();
                if t.line == target_line && target_col >= t.col && target_col < t.col + len {
                    let (sl, sc) = char_to_lsp(&text, t.line, t.col);
                    return J::obj()
                        .set("start", lsp_pos(sl, sc))
                        .set("end", lsp_pos(sl, sc + len));
                }
            }
        }
        J::obj()
            .set("start", lsp_pos(line0, col0))
            .set("end", lsp_pos(line0, col0))
    }
}

// ── free helpers ────────────────────────────────────────────────────────

fn symbol_node(name: &str, kind: i64, line1: usize, col1: usize, len: usize, children: Vec<J>) -> J {
    // DocumentSymbol uses range + selectionRange in LSP positions; we don't
    // have the file text here cheaply, so approximate with char-based 0-based
    // (works for ASCII-leading lines; identifiers here are ASCII). The client
    // tolerates slight range imprecision for the outline.
    let pos_s = lsp_pos(line1 - 1, col1 - 1);
    let pos_e = lsp_pos(line1 - 1, col1 - 1 + len);
    let range = J::obj().set("start", pos_s.clone()).set("end", pos_e.clone());
    let mut node = J::obj()
        .set("name", J::Str(name.to_string()))
        .set("kind", J::Num(kind as f64))
        .set("range", range.clone())
        .set("selectionRange", range);
    if !children.is_empty() {
        node = node.set("children", J::Arr(children));
    }
    node
}

fn diag(line0: usize, col_start: usize, col_end: usize, severity: i64, msg: String) -> J {
    J::obj()
        .set(
            "range",
            J::obj()
                .set("start", lsp_pos(line0, col_start))
                .set("end", lsp_pos(line0, col_end)),
        )
        .set("severity", J::Num(severity as f64))
        .set("source", J::Str("evident-lsp".into()))
        .set("message", J::Str(msg))
}

fn lsp_pos(line0: usize, char0: usize) -> J {
    J::obj()
        .set("line", J::Num(line0 as f64))
        .set("character", J::Num(char0 as f64))
}

fn pos_of(v: &J) -> Option<(String, usize, usize)> {
    let p = v.get("params")?;
    let uri = p.get("textDocument")?.get("uri")?.as_str()?.to_string();
    let pos = p.get("position")?;
    let line = pos.get("line")?.as_i64()? as usize;
    let ch = pos.get("character")?.as_i64()? as usize;
    Some((uri, line, ch))
}

fn dedup(mut v: Vec<String>) -> Vec<String> {
    v.dedup();
    v
}

/// Convert a UTF-16 column (LSP) on `line0` to a char column (0-based).
fn utf16_to_char_col(text: &str, line0: usize, utf16col: usize) -> usize {
    let line = text.lines().nth(line0).unwrap_or("");
    let mut u16seen = 0usize;
    let mut chars = 0usize;
    for c in line.chars() {
        if u16seen >= utf16col {
            break;
        }
        u16seen += c.len_utf16();
        chars += 1;
    }
    chars
}

/// Char column (1-based) on `line1` (1-based) → LSP (line0, utf16col).
fn char_to_lsp(text: &str, line1: usize, col1: usize) -> (usize, usize) {
    let line = text.lines().nth(line1 - 1).unwrap_or("");
    let mut u16 = 0usize;
    for (i, c) in line.chars().enumerate() {
        if i + 1 >= col1 {
            break;
        }
        u16 += c.len_utf16();
    }
    (line1 - 1, u16)
}

/// Byte offset in `text` → LSP (line0, utf16col).
fn byte_to_lsp(text: &str, byte: usize) -> (usize, usize) {
    let mut line0 = 0usize;
    let mut col_u16 = 0usize;
    let mut b = 0usize;
    for c in text.chars() {
        if b >= byte {
            break;
        }
        if c == '\n' {
            line0 += 1;
            col_u16 = 0;
        } else {
            col_u16 += c.len_utf16();
        }
        b += c.len_utf8();
    }
    (line0, col_u16)
}

// ── uri/path ────────────────────────────────────────────────────────────

fn path_to_uri(p: &Path) -> String {
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

fn uri_to_path(uri: &str) -> Option<PathBuf> {
    let rest = uri.strip_prefix("file://")?;
    // percent-decode
    let bytes = rest.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            let h = std::str::from_utf8(&bytes[i + 1..i + 3]).ok()?;
            if let Ok(b) = u8::from_str_radix(h, 16) {
                out.push(b);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    Some(PathBuf::from(String::from_utf8(out).ok()?))
}

fn find_root() -> PathBuf {
    let mut dir = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    loop {
        if dir.join("CLAUDE.md").exists() && dir.join("compiler2").exists() {
            return dir;
        }
        if !dir.pop() {
            return std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        }
    }
}

fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
    let rd = match std::fs::read_dir(dir) {
        Ok(r) => r,
        Err(_) => return,
    };
    for ent in rd.flatten() {
        let p = ent.path();
        if p.is_dir() {
            walk(&p, out);
        } else if p.extension().map(|e| e == "ev").unwrap_or(false) {
            out.push(p);
        }
    }
}

// ── stdio framing ───────────────────────────────────────────────────────

fn read_message(stdin: &mut std::io::Stdin) -> Option<String> {
    // read headers
    let mut content_len = 0usize;
    loop {
        let line = read_header_line(stdin)?;
        if line.is_empty() {
            break;
        }
        if let Some(v) = line.strip_prefix("Content-Length:") {
            content_len = v.trim().parse().ok()?;
        }
    }
    if content_len == 0 {
        return None;
    }
    let mut buf = vec![0u8; content_len];
    stdin.read_exact(&mut buf).ok()?;
    String::from_utf8(buf).ok()
}

fn read_header_line(stdin: &mut std::io::Stdin) -> Option<String> {
    let mut line = Vec::new();
    let mut byte = [0u8; 1];
    loop {
        if stdin.read_exact(&mut byte).is_err() {
            return None;
        }
        if byte[0] == b'\n' {
            if line.last() == Some(&b'\r') {
                line.pop();
            }
            return Some(String::from_utf8_lossy(&line).into_owned());
        }
        line.push(byte[0]);
    }
}

fn write_message(body: &str) {
    let out = std::io::stdout();
    let mut h = out.lock();
    let _ = write!(h, "Content-Length: {}\r\n\r\n{}", body.len(), body);
    let _ = h.flush();
}
