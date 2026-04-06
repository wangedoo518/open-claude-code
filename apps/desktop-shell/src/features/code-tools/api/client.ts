import { invoke } from "@tauri-apps/api/core";
import type {
  CodeToolRunResult,
  CodeToolsTerminalConfig,
  RunCodeToolPayload,
} from "@/lib/tauri";

export async function isBinaryExist(binaryName: string): Promise<boolean> {
  return invoke<boolean>("is_binary_exist", { binaryName });
}

export async function installBunBinary(): Promise<void> {
  return invoke<void>("install_bun_binary");
}

export async function getCodeToolAvailableTerminals(): Promise<
  CodeToolsTerminalConfig[]
> {
  return invoke<CodeToolsTerminalConfig[]>("code_tools_get_available_terminals");
}

export async function runCodeTool(
  payload: RunCodeToolPayload
): Promise<CodeToolRunResult> {
  return invoke<CodeToolRunResult>("code_tools_run", {
    payload: {
      cliTool: payload.cliTool,
      directory: payload.directory,
      terminal: payload.terminal,
      autoUpdateToLatest: payload.autoUpdateToLatest,
      environmentVariables: payload.environmentVariables,
      selectedModel: payload.selectedModel,
    },
  });
}
