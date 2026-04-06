export type MessageRole = "user" | "assistant" | "system";
export type MessageType =
  | "text"
  | "tool_use"
  | "tool_result"
  | "todo"
  | "error";

export interface ToolUseData {
  toolName: string;
  toolInput: string;
}

export interface ToolResultData {
  toolName: string;
  output: string;
  isError: boolean;
}

export interface ConversationMessage {
  id: string;
  role: MessageRole;
  type: MessageType;
  content: string;
  timestamp: number;
  toolUse?: ToolUseData;
  toolResult?: ToolResultData;
}
