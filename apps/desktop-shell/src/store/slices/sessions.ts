import { createSlice, type PayloadAction } from "@reduxjs/toolkit";

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

export interface Session {
  id: string;
  title: string;
  projectPath: string;
  model: string;
  createdAt: number;
  updatedAt: number;
  messageCount: number;
}

export interface SessionDetail extends Session {
  messages: ConversationMessage[];
}

interface SessionsState {
  list: Session[];
  activeSessionId: string | null;
  activeMessages: ConversationMessage[];
  isStreaming: boolean;
  streamingContent: string;
}

const initialState: SessionsState = {
  list: [],
  activeSessionId: null,
  activeMessages: [],
  isStreaming: false,
  streamingContent: "",
};

const sessionsSlice = createSlice({
  name: "sessions",
  initialState,
  reducers: {
    setSessions(state, action: PayloadAction<Session[]>) {
      state.list = action.payload;
    },
    addSession(state, action: PayloadAction<Session>) {
      state.list.unshift(action.payload);
    },
    removeSession(state, action: PayloadAction<string>) {
      state.list = state.list.filter((s) => s.id !== action.payload);
      if (state.activeSessionId === action.payload) {
        state.activeSessionId = null;
        state.activeMessages = [];
      }
    },
    setActiveSession(
      state,
      action: PayloadAction<{
        sessionId: string;
        messages: ConversationMessage[];
      }>
    ) {
      state.activeSessionId = action.payload.sessionId;
      state.activeMessages = action.payload.messages;
    },
    appendMessage(state, action: PayloadAction<ConversationMessage>) {
      state.activeMessages.push(action.payload);
    },
    updateLastMessage(state, action: PayloadAction<string>) {
      const last = state.activeMessages[state.activeMessages.length - 1];
      if (last && last.role === "assistant") {
        last.content += action.payload;
      }
    },
    setStreaming(state, action: PayloadAction<boolean>) {
      state.isStreaming = action.payload;
      if (!action.payload) {
        state.streamingContent = "";
      }
    },
    setStreamingContent(state, action: PayloadAction<string>) {
      state.streamingContent = action.payload;
    },
    appendStreamingContent(state, action: PayloadAction<string>) {
      state.streamingContent += action.payload;
    },
    clearMessages(state) {
      state.activeMessages = [];
    },
  },
});

export const {
  setSessions,
  addSession,
  removeSession,
  setActiveSession,
  appendMessage,
  updateLastMessage,
  setStreaming,
  setStreamingContent,
  appendStreamingContent,
  clearMessages,
} = sessionsSlice.actions;
export default sessionsSlice.reducer;
