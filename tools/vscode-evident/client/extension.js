// Minimal VS Code LSP client for Evident. Spawns the `evident-lsp` binary
// over stdio and wires it as the language server for `.ev` files.
//
// Build: see tools/README.md. This client only needs `vscode-languageclient`.
const { workspace, window } = require("vscode");
const { LanguageClient, TransportKind } = require("vscode-languageclient/node");

let client;

function activate(context) {
  const cfg = workspace.getConfiguration("evident");
  const command = cfg.get("lspPath") || "evident-lsp";
  const args = cfg.get("lspArgs") || [];

  const serverOptions = {
    run: { command, args, transport: TransportKind.stdio },
    debug: { command, args, transport: TransportKind.stdio },
  };

  const clientOptions = {
    documentSelector: [{ scheme: "file", language: "evident" }],
    synchronize: {
      fileEvents: workspace.createFileSystemWatcher("**/*.ev"),
    },
  };

  client = new LanguageClient(
    "evident-lsp",
    "Evident Language Server",
    serverOptions,
    clientOptions
  );

  client.start().catch((e) => {
    window.showErrorMessage(
      "evident-lsp failed to start (is the binary on PATH or evident.lspPath set?): " + e
    );
  });
}

function deactivate() {
  return client ? client.stop() : undefined;
}

module.exports = { activate, deactivate };
