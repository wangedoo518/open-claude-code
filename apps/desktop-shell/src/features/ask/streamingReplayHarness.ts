import type { ConversationMessage } from "@/features/common/message-types";

export type ReplayProfileId = "steady" | "mixed" | "burst";

export interface ReplayProfile {
  id: ReplayProfileId;
  label: string;
  intervalMs: number;
  chunkPattern: number[];
}

export interface StreamingReplayRun {
  id: string;
  profile: ReplayProfile;
  messages: ConversationMessage[];
  finalMessage: ConversationMessage;
  content: string;
  totalChars: number;
}

export const REPLAY_PROFILES: ReplayProfile[] = [
  {
    id: "steady",
    label: "Replay steady",
    intervalMs: 24,
    chunkPattern: [18, 24, 32, 42, 56],
  },
  {
    id: "mixed",
    label: "Replay mixed",
    intervalMs: 38,
    chunkPattern: [28, 96, 18, 220, 64, 420, 36, 760],
  },
  {
    id: "burst",
    label: "Replay burst",
    intervalMs: 72,
    chunkPattern: [42, 1400, 64, 2200, 120, 3600, 88, 540],
  },
];

export function getReplayProfile(id: ReplayProfileId): ReplayProfile {
  return REPLAY_PROFILES.find((profile) => profile.id === id) ?? REPLAY_PROFILES[1];
}

export function createStreamingReplayRun(
  profileId: ReplayProfileId,
  startedAt = Date.now(),
): StreamingReplayRun {
  const profile = getReplayProfile(profileId);
  const id = `ask-replay-${profile.id}-${startedAt}`;
  const content = buildReplayContent();
  const messages = buildReplayMessages(id);

  return {
    id,
    profile,
    messages,
    finalMessage: {
      id: `${id}-assistant-final`,
      role: "assistant",
      type: "text",
      content,
      timestamp: startedAt + 50,
      usage: {
        inputTokens: 1680,
        outputTokens: Math.ceil(content.length / 4),
      },
    },
    content,
    totalChars: content.length,
  };
}

export function nextReplayOffset(
  run: StreamingReplayRun,
  currentOffset: number,
  stepIndex: number,
): number {
  const size = run.profile.chunkPattern[stepIndex % run.profile.chunkPattern.length];
  return Math.min(run.totalChars, currentOffset + size);
}

function buildReplayMessages(prefix: string): ConversationMessage[] {
  return [
    {
      id: `${prefix}-user`,
      role: "user",
      type: "text",
      content:
        "Replay a long Claude-style answer with tables, code blocks, tool logs, and mixed-size streaming chunks. Watch for flicker, scroll jump, or final handoff snap.",
      timestamp: 1,
    },
    {
      id: `${prefix}-assistant-setup`,
      role: "assistant",
      type: "text",
      content:
        "I will run a deterministic replay that stresses the same rendering path used by real Ask responses. First I will gather context, then stream a long Markdown response.",
      timestamp: 2,
    },
    toolUse(prefix, "read-ui", "Read", {
      file_path: "apps/desktop-shell/src/features/ask/StreamingMessage.tsx",
    }, 3),
    toolResult(
      prefix,
      "read-ui",
      "Read",
      "Read 212 lines from StreamingMessage.tsx. The component uses AskMarkdown, a streaming cursor, and RAF-based reveal state.",
      false,
      4,
    ),
    toolUse(prefix, "grep-markdown", "Grep", {
      pattern: "AskMarkdown|remark-gfm|ask-stream-body",
      glob: "apps/desktop-shell/src/features/ask/**/*.{ts,tsx}",
    }, 5),
    toolResult(
      prefix,
      "grep-markdown",
      "Grep",
      [
        "AskMarkdown.tsx: import remarkGfm from \"remark-gfm\";",
        "StreamingMessage.tsx: className=\"ask-assistant-prose ask-stream-body\"",
        "globals.css: .ask-stream-body--catching-up::after",
      ].join("\n"),
      false,
      6,
    ),
    {
      id: `${prefix}-assistant-before-smoke`,
      role: "assistant",
      type: "text",
      content:
        "The rendering path is wired. I will run one smoke command before streaming the long replay payload.",
      timestamp: 7,
    },
    toolUse(prefix, "run-smoke", "Bash", {
      command: "npm run build && playwright replay smoke",
    }, 8),
    toolResult(
      prefix,
      "run-smoke",
      "Bash",
      [
        "build: ok",
        "table nodes: 3",
        "tool groups: 2",
        "console errors: 0",
        "scroll jump: below visible threshold",
      ].join("\n"),
      false,
      9,
    ),
  ];
}

function toolUse(
  prefix: string,
  id: string,
  toolName: string,
  input: Record<string, unknown>,
  timestamp: number,
): ConversationMessage {
  return {
    id: `${prefix}-tool-${id}`,
    role: "assistant",
    type: "tool_use",
    content: JSON.stringify(input),
    timestamp,
    toolUse: {
      toolUseId: `${prefix}-${id}`,
      toolName,
      toolInput: JSON.stringify(input, null, 2),
    },
  };
}

function toolResult(
  prefix: string,
  id: string,
  toolName: string,
  output: string,
  isError: boolean,
  timestamp: number,
): ConversationMessage {
  return {
    id: `${prefix}-tool-${id}-result`,
    role: "assistant",
    type: "tool_result",
    content: output,
    timestamp,
    toolResult: {
      toolUseId: `${prefix}-${id}`,
      toolName,
      output,
      isError,
    },
  };
}

function buildReplayContent(): string {
  const sections = [
    `# Long Streaming Replay Report

This replay intentionally mixes prose, compact tables, fenced code, blockquotes, lists, and repeated sections. It is designed to surface the issues that make an assistant UI feel less like Claude: sudden full-text snaps, row-height jumps, unstable Markdown promotion, and noisy tool output.

| Signal | Expected behavior | Failure smell |
|---|---|---|
| Streaming text | Reveals steadily through one transcript shell | Entire answer appears in one flash |
| Markdown table | Promotes to a stable table without reflow spikes | Table remains a paragraph |
| Code block | Keeps a stable frame while streaming | Header/frame pops in late |
| Final handoff | Settles into final message without snapping | Streaming row disappears then reappears |

Compressed table sample: | Input | Renderer | Expected | |---|---|---| | one-line table | AskMarkdown | normalized into rows | | code fence | untouched | byte-safe |`,
    `## Implementation Notes

The replay uses the same visual chain as real Ask turns: \`MessageList\`, \`ToolActionsGroup\`, \`StreamingMessage\`, \`AskMarkdown\`, and \`AskCodeBlock\`. That matters because visual bugs usually hide in integration boundaries, not isolated components.

> A good transcript should feel like it is continuously settling, not swapping between unrelated widgets.`,
    `\`\`\`tsx
type ReplayState = {
  phase: "streaming" | "handoff" | "complete";
  content: string;
  profile: "steady" | "mixed" | "burst";
};

function revealChunk(source: string, offset: number, size: number) {
  return source.slice(0, Math.min(source.length, offset + size));
}
\`\`\``,
  ];

  let index = 1;
  while (sections.join("\n\n").length < 10_500) {
    sections.push(buildRepeatedSection(index));
    index += 1;
  }

  sections.push(`## Replay Conclusion

If this final section appears without a visible snap, the handoff guard is doing its job. The final assistant message can arrive in the session cache while the UI still treats it as the same in-progress transcript, which is the subtle behavior that makes the experience feel more Claude-like.`);

  return sections.join("\n\n");
}

function buildRepeatedSection(index: number): string {
  return `## Observation ${index}: transcript stability

The content in this section is deliberately ordinary. The goal is not to impress the renderer; the goal is to create enough height and Markdown variety for virtualization, auto-scroll, and streaming reveal to work under pressure.

| Check | Pass condition | Notes |
|---|---|---|
| Row height ${index} | no jump while tokens arrive | virtualizer should re-measure smoothly |
| Cursor ${index} | remains at the tail | no detached cursor in tables or code |
| Highlight ${index} | stays subtle | no heavy card feeling |

1. The assistant rail should stay anchored.
2. The action row should remain quiet until hover.
3. Inline code like \`MessageList\`, \`StreamingMessage\`, and \`AskMarkdown\` should read as highlights, not noisy badges.

\`\`\`ts
export const sample${index} = {
  chunkProfile: "mixed",
  expectedRows: ${index + 2},
  shouldSnap: false,
};
\`\`\``;
}
