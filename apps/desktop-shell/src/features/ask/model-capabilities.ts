export interface ModelCapability {
  /** Text chat. Every supported model can do this. */
  chat: true;
  /** OpenAI tool_calls support. Anthropic models go through agentic_loop. */
  tools: boolean;
  /** Streaming tool_calls delta support. */
  streamingTools: boolean;
  /** Parallel tool calls. */
  parallelTools: boolean;
  /** Reasoning / thinking model. */
  reasoning: boolean;
  /** Image input. */
  vision: boolean;
  /** JSON mode / structured output. */
  structuredOutput: boolean;
  /** End-to-end verification status in this app. */
  verificationStatus: "verified" | "documented" | "untested" | "broken";
  /** Estimated context window for display. */
  contextWindow: number;
  /** Extra note shown in the capability hover card. */
  note?: string;
}

export type CapabilityProviderKind = "anthropic" | "openai_compat";

const OPENAI_COMPAT_TOOL_LOOP_NOTE =
  "后端已开放 OpenAI compat 工具调用；默认只暴露安全只读工具集，尚未完成该模型实测。";

export const MODEL_CAPABILITIES: Record<string, ModelCapability> = {
  "deepseek-chat": {
    chat: true,
    tools: true,
    streamingTools: true,
    parallelTools: true,
    reasoning: false,
    vision: false,
    structuredOutput: true,
    verificationStatus: "verified",
    contextWindow: 64000,
    note: "DeepSeek-V3 + Step 4.4-4.7 multi-turn agentic loop 已接入。默认安全工具集。",
  },
  "deepseek-v3": {
    chat: true,
    tools: true,
    streamingTools: true,
    parallelTools: false,
    reasoning: false,
    vision: false,
    structuredOutput: true,
    verificationStatus: "untested",
    contextWindow: 64000,
    note: OPENAI_COMPAT_TOOL_LOOP_NOTE,
  },
  "deepseek-v3.1": {
    chat: true,
    tools: true,
    streamingTools: true,
    parallelTools: false,
    reasoning: false,
    vision: false,
    structuredOutput: true,
    verificationStatus: "untested",
    contextWindow: 64000,
    note: OPENAI_COMPAT_TOOL_LOOP_NOTE,
  },
  "deepseek-reasoner": {
    chat: true,
    tools: false,
    streamingTools: false,
    parallelTools: false,
    reasoning: true,
    vision: false,
    structuredOutput: false,
    verificationStatus: "documented",
    contextWindow: 64000,
    note: "DeepSeek-R1 推理模型。模型本身不支持 tool_calls。",
  },
  "moonshot-v1-128k": {
    chat: true,
    tools: true,
    streamingTools: true,
    parallelTools: false,
    reasoning: false,
    vision: false,
    structuredOutput: true,
    verificationStatus: "untested",
    contextWindow: 128000,
    note: `Moonshot Kimi 长上下文文本模型。${OPENAI_COMPAT_TOOL_LOOP_NOTE}`,
  },
  "kimi-k2": {
    chat: true,
    tools: true,
    streamingTools: true,
    parallelTools: false,
    reasoning: false,
    vision: false,
    structuredOutput: true,
    verificationStatus: "untested",
    contextWindow: 200000,
    note: `Kimi K2 系列。${OPENAI_COMPAT_TOOL_LOOP_NOTE}`,
  },
  "qwen-plus": {
    chat: true,
    tools: true,
    streamingTools: true,
    parallelTools: false,
    reasoning: false,
    vision: false,
    structuredOutput: true,
    verificationStatus: "untested",
    contextWindow: 32000,
    note: `通义千问 Plus。${OPENAI_COMPAT_TOOL_LOOP_NOTE}`,
  },
  "qwen-max": {
    chat: true,
    tools: true,
    streamingTools: true,
    parallelTools: false,
    reasoning: false,
    vision: false,
    structuredOutput: true,
    verificationStatus: "untested",
    contextWindow: 32000,
    note: `通义千问 Max。${OPENAI_COMPAT_TOOL_LOOP_NOTE}`,
  },
  "gpt-4o": {
    chat: true,
    tools: true,
    streamingTools: true,
    parallelTools: false,
    reasoning: false,
    vision: true,
    structuredOutput: true,
    verificationStatus: "untested",
    contextWindow: 128000,
    note: "OpenAI GPT-4o。官方支持视觉；本产品图像输入另行接入。后端已开放工具调用，未实测。",
  },
  "glm-4-plus": {
    chat: true,
    tools: true,
    streamingTools: true,
    parallelTools: false,
    reasoning: false,
    vision: false,
    structuredOutput: true,
    verificationStatus: "untested",
    contextWindow: 128000,
    note: `GLM-4 Plus。${OPENAI_COMPAT_TOOL_LOOP_NOTE}`,
  },
  "grok-3": {
    chat: true,
    tools: true,
    streamingTools: true,
    parallelTools: false,
    reasoning: false,
    vision: false,
    structuredOutput: false,
    verificationStatus: "untested",
    contextWindow: 128000,
    note: `Grok 3。${OPENAI_COMPAT_TOOL_LOOP_NOTE}`,
  },
  "claude-opus-4-6": {
    chat: true,
    tools: true,
    streamingTools: true,
    parallelTools: true,
    reasoning: false,
    vision: true,
    structuredOutput: true,
    verificationStatus: "verified",
    contextWindow: 200000,
    note: "Claude Opus 4.6，走 agentic_loop 路径，工具调用完整可用。",
  },
  "claude-sonnet-4-5": {
    chat: true,
    tools: true,
    streamingTools: true,
    parallelTools: true,
    reasoning: false,
    vision: true,
    structuredOutput: true,
    verificationStatus: "documented",
    contextWindow: 200000,
  },
};

export const DEFAULT_CAPABILITY: ModelCapability = {
  chat: true,
  tools: false,
  streamingTools: false,
  parallelTools: false,
  reasoning: false,
  vision: false,
  structuredOutput: false,
  verificationStatus: "untested",
  contextWindow: 4096,
};

export function getModelCapability(
  modelId: string,
  providerKind: CapabilityProviderKind,
): ModelCapability {
  const normalized = modelId.trim().toLowerCase();
  if (MODEL_CAPABILITIES[normalized]) {
    return MODEL_CAPABILITIES[normalized];
  }

  const knownEntries = Object.entries(MODEL_CAPABILITIES).sort(
    ([a], [b]) => b.length - a.length,
  );
  for (const [knownId, capability] of knownEntries) {
    const prefix = knownId.split("-").slice(0, 2).join("-");
    if (prefix && normalized.startsWith(prefix)) {
      return capability;
    }
  }

  if (providerKind === "anthropic") {
    return {
      ...DEFAULT_CAPABILITY,
      tools: true,
      streamingTools: true,
      parallelTools: true,
      vision: true,
      verificationStatus: "untested",
      note: "未知 Anthropic 模型，根据 provider 类型推断支持工具。",
    };
  }

  return {
    ...DEFAULT_CAPABILITY,
    note: "未知 OpenAI 兼容模型，能力未验证。",
  };
}

export function formatContextWindow(size: number): string {
  if (size >= 1000) {
    return `${Math.round(size / 1000)}k`;
  }
  return String(size);
}
