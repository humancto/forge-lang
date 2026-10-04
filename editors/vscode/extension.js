// Forge language support for VS Code.
//
// Plain CommonJS (no build step). Starts the Forge language server
// (`forge lsp`) over stdio and registers the `forge` debug type, which runs
// the Forge debug adapter (`forge dap`).
"use strict";

const vscode = require("vscode");
const { LanguageClient, TransportKind } = require("vscode-languageclient/node");

/** @type {LanguageClient | undefined} */
let client;
/** @type {vscode.OutputChannel | undefined} */
let outputChannel;

function forgeConfig() {
  return vscode.workspace.getConfiguration("forge");
}

/** Path to the `forge` executable from settings (default: `forge` on PATH). */
function forgePath() {
  const configured = forgeConfig().get("path", "forge");
  return (configured && configured.trim()) || "forge";
}

async function startClient() {
  if (!forgeConfig().get("lsp.enabled", true)) {
    return;
  }
  const command = forgePath();
  /** @type {import("vscode-languageclient/node").ServerOptions} */
  const serverOptions = {
    run: { command, args: ["lsp"], transport: TransportKind.stdio },
    debug: { command, args: ["lsp"], transport: TransportKind.stdio },
  };
  /** @type {import("vscode-languageclient/node").LanguageClientOptions} */
  const clientOptions = {
    documentSelector: [
      { scheme: "file", language: "forge" },
      { scheme: "untitled", language: "forge" },
    ],
    outputChannel,
  };
  client = new LanguageClient("forge", "Forge Language Server", serverOptions, clientOptions);
  try {
    await client.start();
  } catch (err) {
    client = undefined;
    const choice = await vscode.window.showErrorMessage(
      `Forge: could not start the language server with "${command} lsp" (${err && err.message ? err.message : err}). ` +
        "Install Forge or set \"forge.path\" to the forge executable.",
      "Open Settings"
    );
    if (choice === "Open Settings") {
      vscode.commands.executeCommand("workbench.action.openSettings", "forge.path");
    }
  }
}

async function stopClient() {
  if (client) {
    const running = client;
    client = undefined;
    try {
      await running.stop();
    } catch (_) {
      // The server may already have exited; nothing to clean up.
    }
  }
}

async function restartClient() {
  await stopClient();
  await startClient();
}

/** Launches `forge dap` for every `forge` debug session. */
class ForgeDebugAdapterFactory {
  createDebugAdapterDescriptor() {
    return new vscode.DebugAdapterExecutable(forgePath(), ["dap"]);
  }
}

/** Lets F5 work without a launch.json by debugging the active Forge file. */
class ForgeDebugConfigurationProvider {
  resolveDebugConfiguration(_folder, config) {
    if (!config.type && !config.request && !config.name) {
      const editor = vscode.window.activeTextEditor;
      if (editor && editor.document.languageId === "forge") {
        config.type = "forge";
        config.name = "Debug current Forge file";
        config.request = "launch";
        config.program = "${file}";
        config.stopOnEntry = false;
      }
    }
    if (!config.program) {
      return vscode.window
        .showInformationMessage("Forge: no program to debug. Open a .fg file or set \"program\" in launch.json.")
        .then(() => undefined);
    }
    return config;
  }
}

/** @param {vscode.ExtensionContext} context */
async function activate(context) {
  outputChannel = vscode.window.createOutputChannel("Forge");
  context.subscriptions.push(outputChannel);

  context.subscriptions.push(
    vscode.commands.registerCommand("forge.restartServer", restartClient),
    vscode.workspace.onDidChangeConfiguration((event) => {
      if (event.affectsConfiguration("forge.path") || event.affectsConfiguration("forge.lsp.enabled")) {
        restartClient();
      }
    }),
    vscode.debug.registerDebugAdapterDescriptorFactory("forge", new ForgeDebugAdapterFactory()),
    vscode.debug.registerDebugConfigurationProvider("forge", new ForgeDebugConfigurationProvider())
  );

  await startClient();
}

function deactivate() {
  return stopClient();
}

module.exports = { activate, deactivate };
