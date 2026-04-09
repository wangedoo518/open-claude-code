// S0.3 extraction target: permission request wire shape.
//
// Original: features/session-workbench/permission-types.ts. Kept as a
// standalone module so the Composer (features/ask) and the
// WikiPermissionDialog (features/permission) can share the contract
// without pulling in either feature's full component surface.

export type PermissionAction = "allow" | "deny" | "allow_always";

export interface PermissionRequest {
  id: string;
  toolName: string;
  toolInput: Record<string, unknown>;
  riskLevel: "low" | "medium" | "high";
  description?: string;
}
