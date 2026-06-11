//! evident-lsp — a production Language Server for the Evident constraint
//! language, built on `tower-lsp` (async JSON-RPC over stdio).
//!
//! It reuses the `evident-tools` engine as a library (lexer / index / rename /
//! resolve / positions) — the refactoring logic is NOT duplicated here. The
//! server adds: incremental document sync, a workspace index, UTF-16-correct
//! position conversion, diagnostics push, and the full set of navigation /
//! edit capabilities listed in `capabilities()`.
//!
//! Any LSP-conformant editor (VS Code, Neovim, Helix, Zed, Emacs, …) drives it
//! over stdio. See tools/evident-lsp/README.md for per-editor wiring.

mod document;
mod workspace;

use std::path::PathBuf;
use std::sync::Mutex;

use evident_tools::index::{self, DeclKind, Index, RefKind};
use evident_tools::positions::LineIndex;
use evident_tools::rename as engine_rename;
use evident_tools::resolve;

use document::DocStore;
use tower_lsp::jsonrpc::Result as RpcResult;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer, LspService, Server};

struct Backend {
    client: Client,
    docs: Mutex<DocStore>,
    /// Workspace root (discovered at initialize from the workspace folders or
    /// CWD). All on-disk indexing is rooted here.
    root: Mutex<PathBuf>,
}

impl Backend {
    fn new(client: Client) -> Self {
        Backend {
            client,
            docs: Mutex::new(DocStore::new()),
            root: Mutex::new(std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))),
        }
    }

    fn root(&self) -> PathBuf {
        self.root.lock().unwrap().clone()
    }

    /// Snapshot of open-buffer (path, text) overrides for index overlay.
    fn overrides(&self) -> Vec<(PathBuf, String)> {
        let docs = self.docs.lock().unwrap();
        docs.iter()
            .filter_map(|(uri, text)| uri_to_path(uri).map(|p| (p, text.clone())))
            .collect()
    }

    /// Build a fresh workspace index (disk + open-buffer overlay).
    fn index(&self) -> Index {
        workspace::build_workspace_index(&self.root(), &self.overrides())
    }

    /// Current text of `uri` (open buffer preferred, else on disk).
    fn text_of(&self, uri: &Url) -> Option<String> {
        if let Some(t) = self.docs.lock().unwrap().get(uri) {
            return Some(t.clone());
        }
        uri_to_path(uri).and_then(|p| std::fs::read_to_string(p).ok())
    }

    /// Current text of a path (open buffer preferred, else on disk). Used to
    /// convert occurrence positions exactly even for unsaved buffers.
    fn text_for_path(&self, path: &std::path::Path) -> Option<String> {
        if let Some(uri) = path_to_uri(path) {
            if let Some(t) = self.docs.lock().unwrap().get(&uri) {
                return Some(t.clone());
            }
        }
        std::fs::read_to_string(path).ok()
    }

    /// Resolve the identifier at an LSP position in `uri`.
    fn ident_at(&self, uri: &Url, pos: Position) -> Option<(String, resolve::IdentAt, String)> {
        let text = self.text_of(uri)?;
        let li = LineIndex::new(&text);
        let (line1, col1) = lsp_to_internal(&li, &text, pos);
        let id = resolve::ident_at(&text, line1, col1)?;
        Some((id.base.clone(), id, text))
    }

    async fn publish_diagnostics(&self, uri: Url) {
        let text = match self.text_of(&uri) {
            Some(t) => t,
            None => return,
        };
        let path = uri_to_path(&uri).unwrap_or_else(|| PathBuf::from("doc.ev"));
        let li = LineIndex::new(&text);
        let diags = resolve::diagnostics(&path, &text)
            .into_iter()
            .map(|d| {
                let start = li.char_to_lsp(d.line, d.col_start);
                let end = li.char_to_lsp(d.line, d.col_end);
                Diagnostic {
                    range: Range::new(
                        Position::new(start.0 as u32, start.1 as u32),
                        Position::new(end.0 as u32, end.1 as u32),
                    ),
                    severity: Some(match d.severity {
                        1 => DiagnosticSeverity::ERROR,
                        2 => DiagnosticSeverity::WARNING,
                        3 => DiagnosticSeverity::INFORMATION,
                        _ => DiagnosticSeverity::HINT,
                    }),
                    code: Some(NumberOrString::String(d.code)),
                    source: Some("evident-lsp".to_string()),
                    message: d.message,
                    ..Default::default()
                }
            })
            .collect();
        self.client.publish_diagnostics(uri, diags, None).await;
    }
}

#[tower_lsp::async_trait]
impl LanguageServer for Backend {
    async fn initialize(&self, params: InitializeParams) -> RpcResult<InitializeResult> {
        // Determine the workspace root: first workspace folder, else rootUri,
        // else CWD; then walk up to the Evident repo marker.
        let start = params
            .workspace_folders
            .as_ref()
            .and_then(|f| f.first())
            .and_then(|f| uri_to_path(&f.uri))
            .or_else(|| {
                #[allow(deprecated)]
                params.root_uri.as_ref().and_then(uri_to_path)
            })
            .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));
        let root = workspace::find_root(&start);
        *self.root.lock().unwrap() = root;

        Ok(InitializeResult {
            capabilities: ServerCapabilities {
                text_document_sync: Some(TextDocumentSyncCapability::Kind(
                    TextDocumentSyncKind::INCREMENTAL,
                )),
                definition_provider: Some(OneOf::Left(true)),
                references_provider: Some(OneOf::Left(true)),
                document_highlight_provider: Some(OneOf::Left(true)),
                hover_provider: Some(HoverProviderCapability::Simple(true)),
                document_symbol_provider: Some(OneOf::Left(true)),
                workspace_symbol_provider: Some(OneOf::Left(true)),
                rename_provider: Some(OneOf::Right(RenameOptions {
                    prepare_provider: Some(true),
                    work_done_progress_options: Default::default(),
                })),
                completion_provider: Some(CompletionOptions {
                    trigger_characters: Some(vec![".".to_string()]),
                    ..Default::default()
                }),
                folding_range_provider: Some(FoldingRangeProviderCapability::Simple(true)),
                ..Default::default()
            },
            server_info: Some(ServerInfo {
                name: "evident-lsp".to_string(),
                version: Some(env!("CARGO_PKG_VERSION").to_string()),
            }),
        })
    }

    async fn initialized(&self, _: InitializedParams) {
        self.client
            .log_message(
                MessageType::INFO,
                format!("evident-lsp ready (root: {})", self.root().display()),
            )
            .await;
    }

    async fn shutdown(&self) -> RpcResult<()> {
        Ok(())
    }

    // ── document sync ───────────────────────────────────────────────────

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        let uri = params.text_document.uri.clone();
        self.docs
            .lock()
            .unwrap()
            .open(uri.clone(), params.text_document.text);
        self.publish_diagnostics(uri).await;
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        let uri = params.text_document.uri.clone();
        let changed = {
            let mut docs = self.docs.lock().unwrap();
            docs.apply_changes(&uri, params.content_changes).is_some()
        };
        if changed {
            self.publish_diagnostics(uri).await;
        }
    }

    async fn did_save(&self, params: DidSaveTextDocumentParams) {
        // Re-publish in case on-disk content matters elsewhere; also handles
        // clients that send full text on save.
        if let Some(text) = params.text {
            self.docs
                .lock()
                .unwrap()
                .open(params.text_document.uri.clone(), text);
        }
        self.publish_diagnostics(params.text_document.uri).await;
    }

    async fn did_close(&self, params: DidCloseTextDocumentParams) {
        self.docs.lock().unwrap().close(&params.text_document.uri);
        // clear diagnostics for the closed doc
        self.client
            .publish_diagnostics(params.text_document.uri, vec![], None)
            .await;
    }

    // ── navigation ──────────────────────────────────────────────────────

    async fn goto_definition(
        &self,
        params: GotoDefinitionParams,
    ) -> RpcResult<Option<GotoDefinitionResponse>> {
        let uri = &params.text_document_position_params.text_document.uri;
        let pos = params.text_document_position_params.position;
        let (name, _id, _text) = match self.ident_at(uri, pos) {
            Some(x) => x,
            None => return Ok(None),
        };
        let idx = self.index();
        let tf = |p: &std::path::Path| self.text_for_path(p);
        let mut locs = Vec::new();
        for o in &idx.occurrences {
            if o.name == name && o.kind.is_decl() {
                if let Some(loc) = occ_location_with(o, &tf) {
                    locs.push(loc);
                }
            }
        }
        if locs.is_empty() {
            Ok(None)
        } else {
            Ok(Some(GotoDefinitionResponse::Array(locs)))
        }
    }

    async fn references(&self, params: ReferenceParams) -> RpcResult<Option<Vec<Location>>> {
        let uri = &params.text_document_position.text_document.uri;
        let pos = params.text_document_position.position;
        let include_decl = params.context.include_declaration;
        let (name, _id, _text) = match self.ident_at(uri, pos) {
            Some(x) => x,
            None => return Ok(None),
        };
        let idx = self.index();
        let tf = |p: &std::path::Path| self.text_for_path(p);
        let mut locs = Vec::new();
        for o in &idx.occurrences {
            if o.name == name {
                if !include_decl && o.kind.is_decl() {
                    continue;
                }
                if let Some(loc) = occ_location_with(o, &tf) {
                    locs.push(loc);
                }
            }
        }
        Ok(Some(locs))
    }

    async fn document_highlight(
        &self,
        params: DocumentHighlightParams,
    ) -> RpcResult<Option<Vec<DocumentHighlight>>> {
        let uri = &params.text_document_position_params.text_document.uri;
        let pos = params.text_document_position_params.position;
        let (name, _id, text) = match self.ident_at(uri, pos) {
            Some(x) => x,
            None => return Ok(None),
        };
        // highlights are within the current document only
        let path = match uri_to_path(uri) {
            Some(p) => p,
            None => return Ok(None),
        };
        let mut idx = Index::default();
        index::index_file(&path, &text, &mut idx);
        let li = LineIndex::new(&text);
        let mut hs = Vec::new();
        for o in &idx.occurrences {
            if o.name == name {
                let len = o.name.chars().count() + if o.is_dual { 1 } else { 0 };
                let (sl, sc) = li.char_to_lsp(o.line, o.col);
                let kind = if o.kind.is_decl() {
                    DocumentHighlightKind::WRITE
                } else if o.kind == RefKind::AssignLhs {
                    DocumentHighlightKind::WRITE
                } else {
                    DocumentHighlightKind::READ
                };
                hs.push(DocumentHighlight {
                    range: Range::new(
                        Position::new(sl as u32, sc as u32),
                        Position::new(sl as u32, (sc + len) as u32),
                    ),
                    kind: Some(kind),
                });
            }
        }
        Ok(Some(hs))
    }

    async fn hover(&self, params: HoverParams) -> RpcResult<Option<Hover>> {
        let uri = &params.text_document_position_params.text_document.uri;
        let pos = params.text_document_position_params.position;
        let (name, id, _text) = match self.ident_at(uri, pos) {
            Some(x) => x,
            None => return Ok(None),
        };
        let idx = self.index();
        let mut lines: Vec<String> = Vec::new();
        let mut seen = std::collections::BTreeSet::new();
        for d in &idx.decls {
            if d.name == name {
                let hdr = if d.header_slots.is_empty() {
                    String::new()
                } else {
                    format!("({})", d.header_slots.join(", "))
                };
                let sig = format!("{} {}{}", d.kind.label(), d.name, hdr);
                if seen.insert(sig.clone()) {
                    lines.push(sig);
                }
            }
        }
        if lines.is_empty() {
            let nrefs = idx.occurrences.iter().filter(|o| o.name == name).count();
            let dual = if id.is_dual { " (carry dual)" } else { "" };
            lines.push(format!(
                "variable `{}`{} — {} occurrence(s) tree-wide",
                name, dual, nrefs
            ));
        }
        let md = format!("```evident\n{}\n```", lines.join("\n"));
        Ok(Some(Hover {
            contents: HoverContents::Markup(MarkupContent {
                kind: MarkupKind::Markdown,
                value: md,
            }),
            range: None,
        }))
    }

    // ── symbols ─────────────────────────────────────────────────────────

    async fn document_symbol(
        &self,
        params: DocumentSymbolParams,
    ) -> RpcResult<Option<DocumentSymbolResponse>> {
        let uri = &params.text_document.uri;
        let path = match uri_to_path(uri) {
            Some(p) => p,
            None => return Ok(None),
        };
        let text = self.text_of(uri).unwrap_or_default();
        let li = LineIndex::new(&text);
        let mut idx = Index::default();
        index::index_file(&path, &text, &mut idx);

        let mut decls = idx.decls.clone();
        decls.sort_by_key(|d| (d.line, d.col));
        let mut syms = Vec::new();
        for d in &decls {
            if d.kind == DeclKind::EnumVariant {
                continue; // nested under their enum
            }
            let kind = match d.kind {
                DeclKind::Enum => SymbolKind::ENUM,
                DeclKind::Type | DeclKind::Schema => SymbolKind::STRUCT,
                DeclKind::Claim | DeclKind::Fsm => SymbolKind::CLASS,
                DeclKind::Subclaim => SymbolKind::METHOD,
                DeclKind::EnumVariant => SymbolKind::ENUM_MEMBER,
            };
            let mut children = Vec::new();
            for o in &idx.occurrences {
                if o.scope == d.name
                    && matches!(o.kind, RefKind::MemberDecl | RefKind::HeaderSlot)
                {
                    children.push(make_symbol(
                        &li,
                        &o.name,
                        SymbolKind::FIELD,
                        o.line,
                        o.col,
                        vec![],
                    ));
                }
            }
            if d.kind == DeclKind::Enum {
                for o in &idx.occurrences {
                    if o.scope == d.name && o.kind == RefKind::VariantDecl {
                        children.push(make_symbol(
                            &li,
                            &o.name,
                            SymbolKind::ENUM_MEMBER,
                            o.line,
                            o.col,
                            vec![],
                        ));
                    }
                }
            }
            let hdr = if d.header_slots.is_empty() {
                String::new()
            } else {
                format!("({})", d.header_slots.join(", "))
            };
            let mut sym = make_symbol(&li, &d.name, kind, d.line, d.col, children);
            if !hdr.is_empty() {
                sym.detail = Some(hdr);
            }
            syms.push(sym);
        }
        Ok(Some(DocumentSymbolResponse::Nested(syms)))
    }

    async fn symbol(
        &self,
        params: WorkspaceSymbolParams,
    ) -> RpcResult<Option<Vec<SymbolInformation>>> {
        let q = params.query.to_lowercase();
        let idx = self.index();
        let mut out = Vec::new();
        for d in &idx.decls {
            if d.kind == DeclKind::EnumVariant && !q.is_empty() && !d.name.to_lowercase().contains(&q)
            {
                continue;
            }
            if !q.is_empty() && !d.name.to_lowercase().contains(&q) {
                continue;
            }
            let kind = match d.kind {
                DeclKind::Enum => SymbolKind::ENUM,
                DeclKind::Type | DeclKind::Schema => SymbolKind::STRUCT,
                DeclKind::Claim | DeclKind::Fsm => SymbolKind::CLASS,
                DeclKind::Subclaim => SymbolKind::METHOD,
                DeclKind::EnumVariant => SymbolKind::ENUM_MEMBER,
            };
            let text = std::fs::read_to_string(&d.file).unwrap_or_default();
            let li = LineIndex::new(&text);
            let (sl, sc) = li.char_to_lsp(d.line, d.col);
            let len = d.name.chars().count();
            let uri = match path_to_uri(&d.file) {
                Some(u) => u,
                None => continue,
            };
            #[allow(deprecated)]
            out.push(SymbolInformation {
                name: d.name.clone(),
                kind,
                tags: None,
                deprecated: None,
                location: Location {
                    uri,
                    range: Range::new(
                        Position::new(sl as u32, sc as u32),
                        Position::new(sl as u32, (sc + len) as u32),
                    ),
                },
                container_name: None,
            });
        }
        Ok(Some(out))
    }

    // ── completion ──────────────────────────────────────────────────────

    async fn completion(&self, params: CompletionParams) -> RpcResult<Option<CompletionResponse>> {
        let uri = &params.text_document_position.text_document.uri;
        let pos = params.text_document_position.position;
        let text = self.text_of(uri).unwrap_or_default();
        // compute the partial word prefix before the cursor
        let prefix = word_prefix_before(&text, pos);
        let idx = self.index();
        let items = resolve::completions(&idx, &prefix)
            .into_iter()
            .map(|c| CompletionItem {
                label: c.label,
                kind: Some(completion_kind(c.kind)),
                detail: if c.detail.is_empty() {
                    None
                } else {
                    Some(c.detail)
                },
                ..Default::default()
            })
            .collect();
        Ok(Some(CompletionResponse::Array(items)))
    }

    // ── folding ─────────────────────────────────────────────────────────

    async fn folding_range(
        &self,
        params: FoldingRangeParams,
    ) -> RpcResult<Option<Vec<FoldingRange>>> {
        let uri = &params.text_document.uri;
        let path = match uri_to_path(uri) {
            Some(p) => p,
            None => return Ok(None),
        };
        let text = self.text_of(uri).unwrap_or_default();
        let mut idx = Index::default();
        index::index_file(&path, &text, &mut idx);
        // fold each top-level decl from its line to the line before the next
        // top-level decl (or EOF).
        let mut decls: Vec<_> = idx
            .decls
            .iter()
            .filter(|d| d.kind != DeclKind::EnumVariant && d.col == 1)
            .collect();
        decls.sort_by_key(|d| d.line);
        let total_lines = text.lines().count();
        let mut out = Vec::new();
        for (i, d) in decls.iter().enumerate() {
            let end = if i + 1 < decls.len() {
                decls[i + 1].line.saturating_sub(2)
            } else {
                total_lines.saturating_sub(1)
            };
            if end + 1 > d.line {
                out.push(FoldingRange {
                    start_line: (d.line - 1) as u32,
                    end_line: end as u32,
                    start_character: None,
                    end_character: None,
                    kind: Some(FoldingRangeKind::Region),
                    collapsed_text: None,
                });
            }
        }
        Ok(Some(out))
    }

    // ── rename ──────────────────────────────────────────────────────────

    async fn prepare_rename(
        &self,
        params: TextDocumentPositionParams,
    ) -> RpcResult<Option<PrepareRenameResponse>> {
        let uri = &params.text_document.uri;
        let pos = params.position;
        let (_name, id, text) = match self.ident_at(uri, pos) {
            Some(x) => x,
            None => return Ok(None),
        };
        let li = LineIndex::new(&text);
        let (sl, sc) = li.char_to_lsp(id.line, id.col);
        Ok(Some(PrepareRenameResponse::RangeWithPlaceholder {
            range: Range::new(
                Position::new(sl as u32, sc as u32),
                Position::new(sl as u32, (sc + id.len_chars) as u32),
            ),
            placeholder: if id.is_dual {
                format!("_{}", id.base)
            } else {
                id.base.clone()
            },
        }))
    }

    async fn rename(&self, params: RenameParams) -> RpcResult<Option<WorkspaceEdit>> {
        let uri = &params.text_document_position.text_document.uri;
        let pos = params.text_document_position.position;
        let new_name = params.new_name;
        let (old, _id, _text) = match self.ident_at(uri, pos) {
            Some(x) => x,
            None => return Ok(None),
        };
        let new_base = new_name
            .strip_prefix('_')
            .filter(|r| !r.is_empty())
            .unwrap_or(&new_name)
            .to_string();

        if !engine_rename::valid_ident(&new_base) {
            return Err(tower_lsp::jsonrpc::Error::invalid_params(format!(
                "`{new_base}` is not a valid Evident identifier"
            )));
        }
        if old == new_base {
            return Ok(None);
        }
        let files = workspace::load_files_with_overlay(&self.root(), &self.overrides());

        // Collision guard — the merge trap (tools/README.md trap #3). Renaming
        // onto an existing name silently MERGES two distinct symbols under
        // names-match composition, which can change semantics / explode the
        // solver. We REFUSE with a protocol error (not a silent edit).
        let target_count = engine_rename::count_base(&files, &new_base);
        if target_count > 0 {
            return Err(tower_lsp::jsonrpc::Error::invalid_params(format!(
                "refusing rename '{old}' → '{new_base}': target already occurs {target_count} \
                 time(s) — this would MERGE two distinct symbols under names-match composition. \
                 Use the `evt rename --force` CLI if this is intended."
            )));
        }

        let edits = engine_rename::compute(&files, &old, &new_base);
        let mut changes: std::collections::HashMap<Url, Vec<TextEdit>> =
            std::collections::HashMap::new();
        for fe in &edits {
            // Use the open-buffer text if present so offsets match the client.
            let uri = match path_to_uri(&fe.path) {
                Some(u) => u,
                None => continue,
            };
            let text = self
                .text_of(&uri)
                .unwrap_or_else(|| fe.new_content.clone());
            // recompute against the actual current text to be safe
            let recomputed = engine_rename::compute(&[(fe.path.clone(), text.clone())], &old, &new_base);
            let li = LineIndex::new(&text);
            let mut tes = Vec::new();
            if let Some(rfe) = recomputed.first() {
                for e in &rfe.edits {
                    let (sl, sc) = li.byte_to_lsp(e.byte_start);
                    let (el, ec) = li.byte_to_lsp(e.byte_end);
                    tes.push(TextEdit {
                        range: Range::new(
                            Position::new(sl as u32, sc as u32),
                            Position::new(el as u32, ec as u32),
                        ),
                        new_text: e.new_text.clone(),
                    });
                }
            }
            if !tes.is_empty() {
                changes.insert(uri, tes);
            }
        }
        Ok(Some(WorkspaceEdit {
            changes: Some(changes),
            document_changes: None,
            change_annotations: None,
        }))
    }
}

// ── conversions / helpers ────────────────────────────────────────────────

/// LSP position → our internal 1-based (line, char-col).
fn lsp_to_internal(li: &LineIndex, text: &str, pos: Position) -> (usize, usize) {
    let byte = li.lsp_to_byte(pos.line as usize, pos.character as usize);
    // count chars from line start to `byte` for the 1-based char col.
    let line_start_byte = {
        // line start = byte of first char on this line; derive via byte_to_lsp
        // round-trip: the byte at (line, 0)
        li.lsp_to_byte(pos.line as usize, 0)
    };
    let col_chars = text[line_start_byte..byte].chars().count();
    ((pos.line as usize) + 1, col_chars + 1)
}

/// Build an LSP Location for an occurrence. `text_for` resolves a path to its
/// authoritative text (open buffer preferred over disk) so positions are exact
/// even for unsaved edits.
fn occ_location_with<F: Fn(&std::path::Path) -> Option<String>>(
    o: &index::Occurrence,
    text_for: &F,
) -> Option<Location> {
    let text = text_for(&o.file)?;
    let li = LineIndex::new(&text);
    let (sl, sc) = li.char_to_lsp(o.line, o.col);
    let len = o.name.chars().count() + if o.is_dual { 1 } else { 0 };
    let uri = path_to_uri(&o.file)?;
    Some(Location {
        uri,
        range: Range::new(
            Position::new(sl as u32, sc as u32),
            Position::new(sl as u32, (sc + len) as u32),
        ),
    })
}

fn make_symbol(
    li: &LineIndex,
    name: &str,
    kind: SymbolKind,
    line1: usize,
    col1: usize,
    children: Vec<DocumentSymbol>,
) -> DocumentSymbol {
    let (sl, sc) = li.char_to_lsp(line1, col1);
    let len = name.chars().count();
    let range = Range::new(
        Position::new(sl as u32, sc as u32),
        Position::new(sl as u32, (sc + len) as u32),
    );
    #[allow(deprecated)]
    DocumentSymbol {
        name: name.to_string(),
        detail: None,
        kind,
        tags: None,
        deprecated: None,
        range,
        selection_range: range,
        children: if children.is_empty() {
            None
        } else {
            Some(children)
        },
    }
}

fn completion_kind(code: i64) -> CompletionItemKind {
    match code {
        14 => CompletionItemKind::KEYWORD,
        7 => CompletionItemKind::CLASS,
        22 => CompletionItemKind::STRUCT,
        13 => CompletionItemKind::ENUM,
        20 => CompletionItemKind::ENUM_MEMBER,
        5 => CompletionItemKind::FIELD,
        6 => CompletionItemKind::VARIABLE,
        _ => CompletionItemKind::TEXT,
    }
}

/// The partial identifier word immediately left of the cursor (for completion
/// filtering). Empty if the cursor is not after an identifier char.
fn word_prefix_before(text: &str, pos: Position) -> String {
    let li = LineIndex::new(text);
    let byte = li.lsp_to_byte(pos.line as usize, pos.character as usize);
    let before = &text[..byte];
    let mut start = before.len();
    for (i, c) in before.char_indices().rev() {
        if c == '_' || c.is_ascii_alphanumeric() {
            start = i;
        } else {
            break;
        }
    }
    before[start..].to_string()
}

fn path_to_uri(p: &std::path::Path) -> Option<Url> {
    Url::from_file_path(p).ok()
}

fn uri_to_path(uri: &Url) -> Option<PathBuf> {
    uri.to_file_path().ok()
}

#[tokio::main]
async fn main() {
    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();
    let (service, socket) = LspService::new(Backend::new);
    Server::new(stdin, stdout, socket).serve(service).await;
}
