/**
 * Command manifest contract tests.
 *
 * These are authored in the same ambient-vitest style as the existing
 * desktop-shell unit tests: they type-check today and will run as-is once
 * the repo wires Vitest into the package scripts.
 */

import { CLAWWIKI_ROUTES } from "@/shell/clawwiki-routes";
import {
  COMMAND_MANIFEST,
  getCommandById,
  getRouteCommand,
  validateCommandManifest,
} from "./command-manifest";

type TestFn = () => void | Promise<void>;
interface ItFn {
  (name: string, fn: TestFn): void;
}
interface Expect<T> {
  toBe(expected: T): void;
  toBeDefined(): void;
  toContain(expected: unknown): void;
  toEqual(expected: unknown): void;
}
declare const describe: (name: string, fn: () => void) => void;
declare const it: ItFn;
declare const expect: <T>(actual: T) => Expect<T>;

describe("COMMAND_MANIFEST", () => {
  it("has no duplicate ids, test ids, or route drift", () => {
    expect(validateCommandManifest()).toEqual([]);
  });

  it("covers every shell route", () => {
    for (const route of CLAWWIKI_ROUTES) {
      const command = getRouteCommand(route.key);
      expect(command).toBeDefined();
      expect(command?.path).toBe(route.path);
      expect(command?.title).toBe(route.label);
    }
  });

  it("records the global palette shortcut", () => {
    const command = getCommandById("global.openPalette");
    expect(command).toBeDefined();
    expect(command?.shortcut ?? []).toContain("Mod+K");
  });

  it("keeps route command ids stable", () => {
    const routeCommandIds = COMMAND_MANIFEST
      .filter((command) => command.kind === "route")
      .map((command) => command.id);
    expect(routeCommandIds).toContain("route.dashboard");
    expect(routeCommandIds).toContain("route.rules");
    expect(routeCommandIds).toContain("route.connections");
  });
});
