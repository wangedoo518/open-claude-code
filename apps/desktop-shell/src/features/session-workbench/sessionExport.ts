/**
 * Session Export — converts session messages to Markdown or JSON for download.
 */

import type { ConversationMessage } from "./types";
import type { DesktopSessionDetail } from "@/lib/tauri";

/* ─── Markdown export ─────────────────────────────────────────── */

function roleLabel(role: string): string {
  switch (role) {
    case "user":
      return "User";
    case "assistant":
      return "Assistant";
    case "system":
      return "System";
    default:
      return role;
  }
}

function toolUseToMarkdown(msg: ConversationMessage): string {
  const name = msg.toolUse?.toolName ?? "Unknown";
  let inputStr = "";
  if (msg.toolUse?.toolInput) {
    try {
      const parsed =
        typeof msg.toolUse.toolInput === "string"
          ? JSON.parse(msg.toolUse.toolInput)
          : msg.toolUse.toolInput;
      inputStr = JSON.stringify(parsed, null, 2);
    } catch {
      inputStr = String(msg.toolUse.toolInput);
    }
  }
  return `**Tool Call: ${name}**\n\n\`\`\`json\n${inputStr}\n\`\`\``;
}

function toolResultToMarkdown(msg: ConversationMessage): string {
  const name = msg.toolResult?.toolName ?? "Unknown";
  const output = msg.toolResult?.output ?? "";
  const prefix = msg.toolResult?.isError ? " (Error)" : "";
  return `**Tool Result: ${name}${prefix}**\n\n\`\`\`\n${output}\n\`\`\``;
}

export function messagesToMarkdown(
  messages: ConversationMessage[],
  title?: string,
  projectPath?: string
): string {
  const lines: string[] = [];

  // Header
  lines.push(`# ${title ?? "Session Export"}`);
  if (projectPath) {
    lines.push(`> Project: ${projectPath}`);
  }
  lines.push(`> Exported: ${new Date().toISOString()}`);
  lines.push("");
  lines.push("---");
  lines.push("");

  for (const msg of messages) {
    // Role header
    const label = roleLabel(msg.role);

    if (msg.type === "tool_use") {
      lines.push(`### ${label}`);
      lines.push("");
      lines.push(toolUseToMarkdown(msg));
    } else if (msg.type === "tool_result") {
      lines.push(`### ${label}`);
      lines.push("");
      lines.push(toolResultToMarkdown(msg));
    } else {
      lines.push(`### ${label}`);
      lines.push("");
      lines.push(msg.content);
    }

    lines.push("");
    lines.push("---");
    lines.push("");
  }

  return lines.join("\n");
}

/* ─── JSON export ─────────────────────────────────────────────── */

export function messagesToJson(
  messages: ConversationMessage[],
  title?: string,
  projectPath?: string
): string {
  return JSON.stringify(
    {
      title: title ?? "Session Export",
      projectPath: projectPath ?? null,
      exportedAt: new Date().toISOString(),
      messageCount: messages.length,
      messages: messages.map((msg) => ({
        id: msg.id,
        role: msg.role,
        type: msg.type,
        content: msg.content,
        timestamp: msg.timestamp,
        toolUse: msg.toolUse ?? null,
        toolResult: msg.toolResult ?? null,
      })),
    },
    null,
    2
  );
}

/* ─── Raw session JSON export (from Tauri data) ───────────────── */

export function rawSessionToJson(session: DesktopSessionDetail): string {
  return JSON.stringify(
    {
      id: session.id,
      title: session.title,
      projectName: session.project_name,
      projectPath: session.project_path,
      modelLabel: session.model_label,
      environmentLabel: session.environment_label,
      createdAt: session.created_at,
      updatedAt: session.updated_at,
      exportedAt: new Date().toISOString(),
      session: session.session,
    },
    null,
    2
  );
}

/* ─── Download helper ─────────────────────────────────────────── */

export function downloadFile(
  content: string,
  filename: string,
  mimeType: string
) {
  const blob = new Blob([content], { type: mimeType });
  const url = URL.createObjectURL(blob);
  const a = document.createElement("a");
  a.href = url;
  a.download = filename;
  document.body.appendChild(a);
  a.click();
  document.body.removeChild(a);
  URL.revokeObjectURL(url);
}

/* ─── High-level export functions ─────────────────────────────── */

function sanitizeFilename(name: string): string {
  return name.replace(/[^a-zA-Z0-9_-]/g, "_").slice(0, 50);
}

export function exportAsMarkdown(
  messages: ConversationMessage[],
  title?: string,
  projectPath?: string
) {
  const md = messagesToMarkdown(messages, title, projectPath);
  const filename = `${sanitizeFilename(title ?? "session")}_${Date.now()}.md`;
  downloadFile(md, filename, "text/markdown;charset=utf-8");
}

export function exportAsJson(
  messages: ConversationMessage[],
  title?: string,
  projectPath?: string
) {
  const json = messagesToJson(messages, title, projectPath);
  const filename = `${sanitizeFilename(title ?? "session")}_${Date.now()}.json`;
  downloadFile(json, filename, "application/json;charset=utf-8");
}

export function exportRawSession(session: DesktopSessionDetail) {
  const json = rawSessionToJson(session);
  const filename = `${sanitizeFilename(session.title ?? "session")}_raw_${Date.now()}.json`;
  downloadFile(json, filename, "application/json;charset=utf-8");
}
