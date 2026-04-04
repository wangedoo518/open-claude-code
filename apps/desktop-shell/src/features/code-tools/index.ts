import type {
  DesktopManagedProvider,
  DesktopProviderPreset,
  DesktopProviderRuntimeTarget,
} from "@/lib/tauri";

export const CLAUDE_CODE = "claude-code";
export const QWEN_CODE = "qwen-code";
export const GEMINI_CLI = "gemini-cli";
export const OPENAI_CODEX = "openai-codex";
export const IFLOW_CLI = "iflow-cli";
export const GITHUB_COPILOT_CLI = "github-copilot-cli";
export const KIMI_CLI = "kimi-cli";
export const OPENCODE = "opencode";

export const CODE_TOOL_IDS = [
  CLAUDE_CODE,
  QWEN_CODE,
  GEMINI_CLI,
  OPENAI_CODEX,
  IFLOW_CLI,
  GITHUB_COPILOT_CLI,
  KIMI_CLI,
  OPENCODE,
] as const;

export type CodeToolId = (typeof CODE_TOOL_IDS)[number];

export const DEFAULT_CODE_TOOL: CodeToolId = QWEN_CODE;

export interface CodeToolOption {
  value: CodeToolId;
  label: string;
}

export interface CodeToolsProviderModel {
  providerId: string;
  providerName: string;
  providerType: string;
  runtimeTarget: DesktopProviderRuntimeTarget;
  baseUrl: string;
  protocol: string;
  providerLabel: string;
  modelId: string;
  displayName: string;
  contextWindow: number | null;
  maxOutputTokens: number | null;
  billingKind: string | null;
  capabilityTags: string[];
  source: "managed" | "preset";
  managedProviderId: string | null;
  presetId: string | null;
  hasStoredCredential: boolean;
}

export interface SelectedCodeToolModel extends CodeToolsProviderModel {}

export interface CodeToolsProviderEntry {
  id: string;
  name: string;
  providerType: string;
  runtimeTarget: DesktopProviderRuntimeTarget;
  protocol: string;
  baseUrl: string;
  hasStoredCredential: boolean;
  source: "managed" | "preset";
  managedProviderId: string | null;
  presetId: string | null;
  models: CodeToolsProviderModel[];
}

export const CLI_TOOLS: CodeToolOption[] = [
  { value: CLAUDE_CODE, label: "Claude Code" },
  { value: QWEN_CODE, label: "Qwen Code" },
  { value: GEMINI_CLI, label: "Gemini CLI" },
  { value: OPENAI_CODEX, label: "OpenAI Codex" },
  { value: IFLOW_CLI, label: "iFlow CLI" },
  { value: GITHUB_COPILOT_CLI, label: "GitHub Copilot CLI" },
  { value: KIMI_CLI, label: "Kimi CLI" },
  { value: OPENCODE, label: "OpenCode" },
];

function isAllowedCodexProvider(
  providerType: string,
  runtimeTarget: DesktopProviderRuntimeTarget,
  presetId: string | null,
  id: string
) {
  if (runtimeTarget !== "codex") {
    return true;
  }

  return (
    providerType === "codex_openai" ||
    presetId === "codex-openai" ||
    id === "codex-openai"
  );
}

export function getCodeToolModelUniqId(model: SelectedCodeToolModel): string {
  return `${model.providerId}::${model.modelId}`;
}

function isOpenAiCompatible(protocol: string) {
  return protocol === "openai-completions" || protocol === "openai-responses";
}

function isAnthropicCompatible(protocol: string) {
  return protocol === "anthropic-messages";
}

function isGeminiCompatible(protocol: string) {
  return protocol === "gemini";
}

export function filterProvidersForTool(
  providers: CodeToolsProviderEntry[],
  tool: CodeToolId
) {
  return providers.filter((provider) => {
    switch (tool) {
      case CLAUDE_CODE:
        return isAnthropicCompatible(provider.protocol);
      case GEMINI_CLI:
        return isGeminiCompatible(provider.protocol);
      case OPENAI_CODEX:
        return (
          provider.protocol === "openai-responses" &&
          provider.runtimeTarget === "codex" &&
          provider.providerType === "codex_openai"
        );
      case QWEN_CODE:
      case IFLOW_CLI:
      case KIMI_CLI:
        return isOpenAiCompatible(provider.protocol);
      case GITHUB_COPILOT_CLI:
        return false;
      case OPENCODE:
        return (
          isOpenAiCompatible(provider.protocol) ||
          isAnthropicCompatible(provider.protocol)
        );
      default:
        return true;
    }
  });
}

export function parseEnvironmentVariables(envVars: string): Record<string, string> {
  const env: Record<string, string> = {};
  if (!envVars.trim()) {
    return env;
  }

  for (const line of envVars.split("\n")) {
    const trimmedLine = line.trim();
    if (!trimmedLine || !trimmedLine.includes("=")) {
      continue;
    }
    const [key, ...valueParts] = trimmedLine.split("=");
    const trimmedKey = key.trim();
    if (!trimmedKey) {
      continue;
    }
    env[trimmedKey] = valueParts.join("=").trim();
  }

  return env;
}

export function buildCodeToolsProviderCatalog(
  managedProviders: DesktopManagedProvider[],
  presets: DesktopProviderPreset[]
): CodeToolsProviderEntry[] {
  const entries: CodeToolsProviderEntry[] = [];
  const consumedPresetIds = new Set<string>();
  const consumedFingerprints = new Set<string>();

  for (const provider of managedProviders) {
    if (!provider.enabled) {
      continue;
    }
    if (
      !isAllowedCodexProvider(
        provider.provider_type,
        provider.runtime_target,
        provider.preset_id,
        provider.id
      )
    ) {
      continue;
    }
    if (provider.preset_id) {
      consumedPresetIds.add(provider.preset_id);
    }
    consumedFingerprints.add(`${provider.provider_type}::${provider.base_url}`);
    entries.push({
      id: provider.id,
      name: provider.name,
      providerType: provider.provider_type,
      runtimeTarget: provider.runtime_target,
      protocol: provider.protocol,
      baseUrl: provider.base_url,
      hasStoredCredential: provider.has_api_key,
      source: "managed",
      managedProviderId: provider.id,
      presetId: provider.preset_id,
      models: provider.models.map((model) => ({
        providerId: provider.id,
        providerName: provider.name,
        providerType: provider.provider_type,
        runtimeTarget: provider.runtime_target,
        baseUrl: provider.base_url,
        protocol: provider.protocol,
        providerLabel: provider.name,
        modelId: model.model_id,
        displayName: model.display_name,
        contextWindow: model.context_window,
        maxOutputTokens: model.max_output_tokens,
        billingKind: model.billing_kind,
        capabilityTags: model.capability_tags,
        source: "managed",
        managedProviderId: provider.id,
        presetId: provider.preset_id,
        hasStoredCredential: provider.has_api_key,
      })),
    });
  }

  for (const preset of presets) {
    if (
      !isAllowedCodexProvider(
        preset.provider_type,
        preset.runtime_target,
        preset.id,
        preset.id
      )
    ) {
      continue;
    }
    const fingerprint = `${preset.provider_type}::${preset.base_url}`;
    if (consumedPresetIds.has(preset.id) || consumedFingerprints.has(fingerprint)) {
      continue;
    }
    entries.push({
      id: preset.id,
      name: preset.name,
      providerType: preset.provider_type,
      runtimeTarget: preset.runtime_target,
      protocol: preset.protocol,
      baseUrl: preset.base_url,
      hasStoredCredential: false,
      source: "preset",
      managedProviderId: null,
      presetId: preset.id,
      models: preset.models.map((model) => ({
        providerId: preset.id,
        providerName: preset.name,
        providerType: preset.provider_type,
        runtimeTarget: preset.runtime_target,
        baseUrl: preset.base_url,
        protocol: preset.protocol,
        providerLabel: preset.name,
        modelId: model.model_id,
        displayName: model.display_name,
        contextWindow: model.context_window,
        maxOutputTokens: model.max_output_tokens,
        billingKind: model.billing_kind,
        capabilityTags: model.capability_tags,
        source: "preset",
        managedProviderId: null,
        presetId: preset.id,
        hasStoredCredential: false,
      })),
    });
  }

  return entries.sort((left, right) => left.name.localeCompare(right.name));
}
