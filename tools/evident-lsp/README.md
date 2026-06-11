# `evident-lsp` — Language Server for Evident

A production-quality [LSP](https://microsoft.github.io/language-server-protocol/)
server for the Evident constraint language. It speaks standard LSP over stdio,
so it works with **any** conformant editor (VS Code, Neovim, Helix, Zed, Emacs,
Sublime LSP, …).

It is built on [`tower-lsp`](https://crates.io/crates/tower-lsp) (mature async
Rust LSP framework) and **reuses the `evident-tools` engine as a library** — the
lexer, the token-accurate index, the collision-refusing rename, and the
position-conversion code are shared, not duplicated. The hand-rolled std-only
server that previously lived in `evident-tools/src/lsp.rs` is superseded by this
crate (that binary is still buildable as a zero-dependency fallback; see below).

## Capability matrix

| LSP request | Status | Notes |
| --- | --- | --- |
| `initialize` / `initialized` / `shutdown` / `exit` | ✅ | advertises the capabilities below; resolves the workspace root from the first workspace folder / `rootUri` / CWD, walking up to the `CLAUDE.md`+`compiler2/` marker |
| `textDocument/didOpen` / `didChange` / `didClose` / `didSave` | ✅ | **incremental** sync; an in-memory doc store splices ranged changes by byte offset using a UTF-16-correct `LineIndex` |
| `textDocument/publishDiagnostics` | ✅ | pushed on open/change/save: the Seq-membership footgun (`x ∈ xs`, *index-resolved* against `Seq(...)` decls — no false positives on `Set`/types) and capitalised `True`/`False` |
| `textDocument/definition` | ✅ | jumps to a name's declaration(s) (membership `∈`, header slot, top-level decl, enum variant) |
| `textDocument/references` | ✅ | all references incl. the `_x` carry dual; `includeDeclaration` honored |
| `textDocument/documentHighlight` | ✅ | same-document occurrences, read/write classified |
| `textDocument/hover` | ✅ | symbol kind + header signature, or a variable's tree-wide occurrence count (flags carry duals) |
| `textDocument/documentSymbol` | ✅ | outline: claim/fsm/type/enum/subclaim with members + enum variants nested |
| `workspace/symbol` | ✅ | fuzzy (substring) query over all decls |
| `textDocument/completion` | ✅ | keywords, builtin types, claim/type/enum/variant names, member/field names; prefix-filtered |
| `textDocument/prepareRename` | ✅ | returns the token range + placeholder (preserves `_` for duals) |
| `textDocument/rename` | ✅ | reuses the engine's token-accurate, `_x`-dual-aware rename; **refuses a colliding rename with a protocol error** (the names-match merge trap) instead of silently merging |
| `textDocument/foldingRange` | ✅ | folds each top-level decl block |
| `semanticTokens`, `signatureHelp` | ⛔ deferred | see *Limitations* |

### Honest limitations (inherited from the engine)

- **References are name-scoped, not join-resolved.** Evident composition is
  names-match (`..`-lift / bare-mention / `slot ↦ value`); the engine does
  *not* model which same-named tokens are the *same* SMT binding after join
  resolution (that needs the oracle). So `references`/`rename` operate
  tree-wide on the *name* — which is exactly what you want for a consistent
  rename, but can over-report "references to *this* binding" across unrelated
  scopes. This is the deliberate, documented engine contract (see
  `tools/README.md` → "What's robust vs heuristic").
- **The Seq-membership diagnostic is per-document.** It resolves the RHS
  against `Seq(...)` declarations *in the same file*, so a Seq declared in
  another module and used here is not flagged. It never produces false
  positives (`x ∈ Set(T)`, `x ∈ TypeName`, `∀ x ∈ xs` are all clean).
- **No oracle dependency.** Index/refs/rename/symbols/hover/completion/
  diagnostics need no kernel. The authoritative `declare-fun` collision oracle
  is exposed only via the `evt collisions` CLI (it shells out to
  `evident-oracle`); the server's rename guard is the cheap source-level front
  line, matching the CLI's `--force`-gated posture.

## Build

```sh
cd tools/evident-lsp
cargo build --release
# binary: tools/evident-lsp/target/release/evident-lsp
```

`evident-lsp` is its own cargo workspace (like `evident-tools`), so it never
participates in the kernel's build graph. It depends on `evident-tools` via a
sibling path dependency and pulls `tower-lsp` + `tokio` from crates.io.

Put it on PATH if you like:

```sh
export PATH="$PWD/target/release:$PATH"
```

## Test

Protocol-level integration tests spawn the built binary, do the `initialize`
handshake, and drive real requests against actual `compiler2/*.ev` content:

```sh
cd tools/evident-lsp
cargo test
```

Covered: `initialize`→capabilities, `definition`, `references`, a successful
`rename`, a **refused** colliding rename, `documentSymbol`, `hover`,
`completion`, and `publishDiagnostics` on a Seq-membership lint hit. The
`evident-tools` library tests (`cargo test -p evident-tools` from
`tools/evident-tools`) cover the UTF-16 `LineIndex` round-trips and the
diagnostic/resolution logic in isolation.

## Editor wiring

The server reads LSP JSON-RPC on **stdin** and writes on **stdout**; log/trace
goes to the client log channel. Filetype: `.ev`. Below, replace
`/ABS/PATH/.../tools/evident-lsp/target/release/evident-lsp` with your built
binary path.

### VS Code

Use the bundled extension in `tools/vscode-evident/`:

```sh
cd tools/vscode-evident
npm install          # pulls vscode-languageclient
```

Point it at your binary (Settings → `evident.lspPath`, or `.vscode/settings.json`):

```json
{ "evident.lspPath": "/ABS/PATH/tools/evident-lsp/target/release/evident-lsp" }
```

Then press <kbd>F5</kbd> to launch an Extension Development Host, or
`npx vsce package` to build a `.vsix` and `code --install-extension evident-*.vsix`.
The extension also provides TextMate syntax highlighting and the `.ev` language
configuration, and works with any VS Code fork (Cursor, VSCodium, Windsurf).

### Neovim (built-in LSP, 0.8+)

No plugin required — use `vim.lsp.start` from a `ftplugin`/autocommand. First
register the filetype, then start the client:

```lua
-- ~/.config/nvim/init.lua  (or a plugin file)
vim.filetype.add({ extension = { ev = "evident" } })

vim.api.nvim_create_autocmd("FileType", {
  pattern = "evident",
  callback = function(args)
    vim.lsp.start({
      name = "evident-lsp",
      cmd = { "/ABS/PATH/tools/evident-lsp/target/release/evident-lsp" },
      root_dir = vim.fs.dirname(vim.fs.find({ "CLAUDE.md", "compiler2" }, {
        upward = true, path = vim.api.nvim_buf_get_name(args.buf),
      })[1]),
    })
  end,
})
```

With **nvim-lspconfig** you can instead register a custom server:

```lua
local configs = require("lspconfig.configs")
local lspconfig = require("lspconfig")
if not configs.evident then
  configs.evident = {
    default_config = {
      cmd = { "/ABS/PATH/tools/evident-lsp/target/release/evident-lsp" },
      filetypes = { "evident" },
      root_dir = lspconfig.util.root_pattern("CLAUDE.md", "compiler2"),
    },
  }
end
vim.filetype.add({ extension = { ev = "evident" } })
lspconfig.evident.setup({})
```

Standard keymaps then work: `gd` (definition), `grr`/`<leader>gr` (references),
`grn` (rename), `K` (hover), `<C-x><C-o>`/blink/cmp (completion). Diagnostics
show inline.

### Helix

Add to `~/.config/helix/languages.toml`:

```toml
[language-server.evident-lsp]
command = "/ABS/PATH/tools/evident-lsp/target/release/evident-lsp"

[[language]]
name = "evident"
scope = "source.evident"
file-types = ["ev"]
comment-token = "--"
language-servers = ["evident-lsp"]
roots = ["CLAUDE.md", "compiler2"]
indent = { tab-width = 4, unit = "    " }
```

`hx --health evident` should then show the server. `gd`, `gr`, `<space>r`
(rename), `<space>s` (document symbols), `<space>S` (workspace symbols) work.

### Zed

Zed needs a small extension to register a language + LSP. Minimal form — create
`~/.config/zed/extensions/evident/` with:

`extension.toml`:

```toml
id = "evident"
name = "Evident"
version = "0.1.0"
schema_version = 1

[language_servers.evident-lsp]
name = "Evident LSP"
languages = ["Evident"]
```

`languages/evident/config.toml`:

```toml
name = "Evident"
grammar = "evident"
path_suffixes = ["ev"]
line_comments = ["-- "]
```

Then in Zed settings (`settings.json`) point the server binary:

```json
{
  "lsp": {
    "evident-lsp": {
      "binary": { "path": "/ABS/PATH/tools/evident-lsp/target/release/evident-lsp" }
    }
  }
}
```

(Zed requires a Tree-sitter grammar named `evident` for full highlighting; the
LSP features — go-to-def, rename, symbols, diagnostics — work via the
`language_servers` registration regardless.)

### Generic stdio LSP client (Emacs eglet/lsp-mode, Sublime LSP, Kakoune, …)

The server is a plain stdio LSP. Any client that lets you specify a command +
filetype works. The essentials:

- **command**: `/ABS/PATH/tools/evident-lsp/target/release/evident-lsp` (no args)
- **filetypes / languageId**: `evident` for files matching `*.ev`
- **transport**: stdio (`Content-Length` framed JSON-RPC)
- **root markers**: `CLAUDE.md`, `compiler2/`

Emacs `eglot` example:

```elisp
(add-to-list 'auto-mode-alist '("\\.ev\\'" . prog-mode))
(with-eval-after-load 'eglot
  (add-to-list 'eglot-server-programs
    '(prog-mode . ("/ABS/PATH/tools/evident-lsp/target/release/evident-lsp")))) ; narrow to an evident-mode if you define one
```

## Fallback: the std-only server

`evident-tools/src/lsp.rs` is the original **zero-dependency** server (hand-rolled
JSON-RPC, no tokio/tower). It implements a subset (definition, references,
documentSymbol, hover, prepareRename/rename, a heuristic Seq diagnostic). If you
cannot pull crates from crates.io, build that one instead:

```sh
cd tools/evident-tools && cargo build --release   # → target/release/evident-lsp
```

Both binaries are named `evident-lsp`; pick whichever your environment allows
and point your editor at its path. The `tower-lsp` build (this crate) is the
recommended, fuller server.
