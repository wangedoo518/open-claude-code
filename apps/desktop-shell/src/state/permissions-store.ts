import { create } from "zustand";
import type {
  PermissionAction,
  PermissionRequest,
} from "@/features/session-workbench/PermissionDialog";

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
  setPendingPermission: (request: PermissionRequest | null) => void;
  resolvePermission: (payload: {
    requestId: string;
    decision: PermissionAction;
  }) => void;
  clearSessionRules: () => void;
  resetPermissions: () => void;
}

export const initialState = {
  pendingRequest: null,
  sessionRules: [],
  decisionLog: [],
} satisfies Pick<
  PermissionsState,
  "pendingRequest" | "sessionRules" | "decisionLog"
>;

export const usePermissionsStore = create<PermissionsState>((set, get) => ({
  ...initialState,
  setPendingPermission: (pendingRequest) => set({ pendingRequest }),
  resolvePermission: ({ requestId, decision }) => {
    const request = get().pendingRequest;

    if (!request || request.id !== requestId) {
      return;
    }

    const nextSessionRules =
      decision === "allow_always"
        ? [
            ...get().sessionRules.filter(
              (rule) => rule.toolName !== request.toolName,
            ),
            {
              toolName: request.toolName,
              behavior: "allow" as const,
            },
          ]
        : get().sessionRules;

    set({
      pendingRequest: null,
      sessionRules: nextSessionRules,
      decisionLog: [
        ...get().decisionLog,
        {
          requestId,
          toolName: request.toolName,
          action: decision,
          timestamp: Date.now(),
        },
      ],
    });
  },
  clearSessionRules: () => set({ sessionRules: [] }),
  resetPermissions: () => set(initialState),
}));

export function shouldAutoAllow(
  rules: PermissionRule[],
  toolName: string,
): boolean {
  return rules.some(
    (rule) => rule.toolName === toolName && rule.behavior === "allow",
  );
}

export function inferToolRiskLevel(
  toolName: string,
): PermissionRequest["riskLevel"] {
  const lower = toolName.toLowerCase();

  if (lower === "bash" || lower === "powershell" || lower.includes("shell")) {
    return "high";
  }

  if (lower === "write" || lower === "writefile") {
    return "medium";
  }

  if (lower === "edit" || lower === "editfile" || lower === "notebookedit") {
    return "medium";
  }

  if (
    lower === "read" ||
    lower === "glob" ||
    lower === "grep" ||
    lower === "toolsearch"
  ) {
    return "low";
  }

  return "medium";
}
