import { createSlice, type PayloadAction } from "@reduxjs/toolkit";
import type {
  PermissionRequest,
  PermissionAction,
} from "@/features/session-workbench/permission-types";

export interface PermissionRule {
  toolName: string;
  ruleContent?: string;
  behavior: "allow" | "deny";
}

export interface PermissionsState {
  pendingRequest: PermissionRequest | null;
  sessionRules: PermissionRule[];
  decisionLog: Array<{
    requestId: string;
    toolName: string;
    action: PermissionAction;
    timestamp: number;
  }>;
}

const initialState: PermissionsState = {
  pendingRequest: null,
  sessionRules: [],
  decisionLog: [],
};

const permissionsSlice = createSlice({
  name: "permissions",
  initialState,
  reducers: {
    setPendingPermission: (
      state,
      action: PayloadAction<PermissionRequest | null>
    ) => {
      state.pendingRequest = action.payload;
    },
    resolvePermission: (
      _state,
      _action: PayloadAction<{
        requestId: string;
        decision: PermissionAction;
      }>
    ) => initialState,
    clearSessionRules: () => initialState,
    resetPermissions() {
      return initialState;
    },
  },
});

export const {
  setPendingPermission,
  resolvePermission,
  clearSessionRules,
  resetPermissions,
} = permissionsSlice.actions;
export default permissionsSlice.reducer;

export function shouldAutoAllow(
  rules: PermissionRule[],
  toolName: string
): boolean {
  return rules.some(
    (rule) => rule.toolName === toolName && rule.behavior === "allow"
  );
}

export function inferToolRiskLevel(
  toolName: string
): PermissionRequest["riskLevel"] {
  const lower = toolName.toLowerCase();

  // High risk: execution, write, destructive
  if (lower === "bash" || lower === "powershell" || lower.includes("shell")) {
    return "high";
  }
  if (lower === "write" || lower === "writefile") {
    return "medium";
  }
  if (lower === "edit" || lower === "editfile" || lower === "notebookedit") {
    return "medium";
  }

  // Low risk: read-only operations
  if (
    lower === "read" ||
    lower === "glob" ||
    lower === "grep" ||
    lower === "toolsearch"
  ) {
    return "low";
  }

  // Default to medium
  return "medium";
}
