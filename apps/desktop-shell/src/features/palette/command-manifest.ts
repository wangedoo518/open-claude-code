import {
  CLAWWIKI_ROUTES,
  type ClawWikiRoute,
  type ClawWikiSection,
} from "@/shell/clawwiki-routes";

export type CommandManifestKind = "global" | "route";

export interface CommandManifestEntry {
  id: string;
  kind: CommandManifestKind;
  title: string;
  section: ClawWikiSection | "global";
  palette: boolean;
  menu: boolean;
  testId: string;
  shortcut?: readonly string[];
  routeKey?: string;
  path?: string;
}

const GLOBAL_COMMANDS: readonly CommandManifestEntry[] = [
  {
    id: "global.openPalette",
    kind: "global",
    title: "打开命令面板",
    section: "global",
    palette: false,
    menu: true,
    shortcut: ["Mod+K"],
    testId: "command.global.openPalette",
  },
  // Slice 46 — Keyboard-first protocol entries 1/27. Browser-style
  // history navigation lives at shell level so every route benefits.
  // The handler is in `ClawWikiShell.tsx`; the manifest entry exists so
  // tests + future menu/palette wiring can reference the same id.
  {
    id: "global.navigateBack",
    kind: "global",
    title: "返回上一个页面",
    section: "global",
    palette: false,
    menu: true,
    shortcut: ["Mod+["],
    testId: "command.global.navigateBack",
  },
  {
    id: "global.navigateForward",
    kind: "global",
    title: "前进到下一个页面",
    section: "global",
    palette: false,
    menu: true,
    shortcut: ["Mod+]"],
    testId: "command.global.navigateForward",
  },
];

function routeToCommand(route: ClawWikiRoute): CommandManifestEntry {
  return {
    id: `route.${route.key}`,
    kind: "route",
    title: route.label,
    section: route.section,
    palette: true,
    menu: route.section !== "hidden",
    routeKey: route.key,
    path: route.path,
    testId: `command.route.${route.key}`,
  };
}

export function buildCommandManifest(
  routes: readonly ClawWikiRoute[] = CLAWWIKI_ROUTES,
): readonly CommandManifestEntry[] {
  return [...GLOBAL_COMMANDS, ...routes.map(routeToCommand)];
}

export const COMMAND_MANIFEST = buildCommandManifest();

export function getCommandById(id: string): CommandManifestEntry | undefined {
  return COMMAND_MANIFEST.find((command) => command.id === id);
}

export function getRouteCommand(routeKey: string): CommandManifestEntry | undefined {
  return COMMAND_MANIFEST.find(
    (command) => command.kind === "route" && command.routeKey === routeKey,
  );
}

export function validateCommandManifest(
  commands: readonly CommandManifestEntry[] = COMMAND_MANIFEST,
  routes: readonly ClawWikiRoute[] = CLAWWIKI_ROUTES,
): string[] {
  const errors: string[] = [];
  const seenIds = new Set<string>();
  const seenTestIds = new Set<string>();

  for (const command of commands) {
    if (seenIds.has(command.id)) {
      errors.push(`duplicate command id: ${command.id}`);
    }
    seenIds.add(command.id);

    if (seenTestIds.has(command.testId)) {
      errors.push(`duplicate command testId: ${command.testId}`);
    }
    seenTestIds.add(command.testId);

    if (command.kind === "route") {
      if (!command.routeKey) errors.push(`route command missing routeKey: ${command.id}`);
      if (!command.path) errors.push(`route command missing path: ${command.id}`);
    }
  }

  for (const route of routes) {
    const command = commands.find(
      (candidate) => candidate.kind === "route" && candidate.routeKey === route.key,
    );
    if (!command) {
      errors.push(`missing route command: ${route.key}`);
      continue;
    }
    if (command.path !== route.path) {
      errors.push(`route command path drift: ${route.key}`);
    }
    if (command.title !== route.label) {
      errors.push(`route command title drift: ${route.key}`);
    }
  }

  return errors;
}
