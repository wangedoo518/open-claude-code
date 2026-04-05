import { createSlice, type PayloadAction } from "@reduxjs/toolkit";
import type {
  PermissionRequest,
  PermissionAction,
} from "@/features/session-workbench/PermissionDialog";

export interface PermissionRule {
  toolName: string;
  ruleContent?: string;
  behavior: "allow" | "deny";
}

export interface PermissionsState {
  /** Current pending permission request (only one at a time) */
  pendingRequest: PermissionRequest | null;
  /** Session-level rules created by "Always allow" / "Always deny" */
  sessionRules: PermissionRule[];
  /** History of decisions made in this session */
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
    setPendingPermission(
      state,
      action: PayloadAction<PermissionRequest | null>
    ) {
      state.pendingRequest = action.payload;
    },
    resolvePermission(
      state,
      action: PayloadAction<{
        requestId: string;
        decision: PermissionAction;
      }>
    ) {
      const { requestId, decision } = action.payload;
      const request = state.pendingRequest;

      if (request && request.id === requestId) {
        // Log the decision
        state.decisionLog.push({
          requestId,
          toolName: request.toolName,
          action: decision,
          timestamp: Date.now(),
        });

        // If "always allow", create a session rule
        if (decision === "allow_always") {
          const existing = state.sessionRules.findIndex(
            (r) => r.toolName === request.toolName
          );
          const rule: PermissionRule = {
            toolName: request.toolName,
            behavior: "allow",
          };
          if (existing >= 0) {
            state.sessionRules[existing] = rule;
          } else {
            state.sessionRules.push(rule);
          }
        }

        // Clear the pending request
        state.pendingRequest = null;
      }
    },
    clearSessionRules(state) {
      state.sessionRules = [];
    },
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

/**
 * Check if a tool should be auto-allowed based on session rules.
 */
export function shouldAutoAllow(
  rules: PermissionRule[],
  toolName: string
): boolean {
  return rules.some(
    (rule) => rule.toolName === toolName && rule.behavior === "allow"
  );
}

/**
 * Determine the risk level for a tool based on its name.
 */
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
