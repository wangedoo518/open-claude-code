#!/usr/bin/env node

import { chromium } from "@playwright/test";
import { mkdir, readFile, writeFile } from "node:fs/promises";
import path from "node:path";
import { ErrorCollector } from "./lib/error-collector.mjs";

const BASE_URL = process.env.BUDDY_SMOKE_URL ?? "http://127.0.0.1:5173/";
const API_BASE = process.env.BUDDY_API_BASE ?? "http://127.0.0.1:4357";
const HEADLESS = process.env.BUDDY_HEADLESS !== "false";
const SMOKE_SLUG = "smoke-edit-page";
const HUNK_SMOKE_PATH = "wiki/concepts/smoke-hunk-discard.md";
const LINE_SMOKE_PATH = "wiki/concepts/smoke-line-discard.md";

const routes = [
  {
    name: "Home / Pulse",
    hash: "/",
    mustContain: ["Home / Pulse", "外脑", "最近 Git 操作"],
  },
  {
    name: "Rules Studio",
    hash: "/rules",
    mustContain: [
      "Rules Studio",
      "Advanced YAML / CodeMirror",
      "Git checkpoint",
      "schema/templates/concept.md",
      "schema/templates/research.md",
      "Root AGENTS.md",
      "schema/CLAUDE.md",
      "schema/policies/maintenance.md",
      "schema/policies/naming.md",
      "Rule file editor",
      "编辑选中文件",
      "Validation snapshot",
      "运行巡检",
    ],
    check: async (page) => {
      const advanced = page
        .locator("details")
        .filter({ hasText: "Advanced YAML / CodeMirror" })
        .first();
      await advanced.waitFor({ state: "attached", timeout: 10_000 });
      const isOpen = await advanced.evaluate((node) => node.open);
      if (isOpen) {
        throw new Error("Rules Studio Advanced CodeMirror panel should be folded by default");
      }
    },
  },
  {
    name: "Connections",
    hash: "/connections",
    mustContain: [
      "连接",
      "Buddy Vault / Git",
      "Remote URL",
      "Remote sync",
      "Pull",
      "Push",
      "保存 origin",
      "丢弃新增行",
      "丢弃 Hunk",
      "丢弃文件",
      "最近 Git 操作",
      "受控自动写入授权",
    ],
  },
  {
    name: "Inbox",
    hash: "/inbox",
    mustContain: ["INBOX", "Vault", "checkpoint"],
  },
  {
    name: "Knowledge",
    hash: "/wiki",
    mustContain: ["知识"],
  },
  {
    name: "Wiki Edit",
    hash: `/wiki/${SMOKE_SLUG}`,
    mustContain: ["Smoke Edit Page", "Original smoke body"],
    check: runWikiEditCheck,
  },
];

function routeUrl(hash) {
  const url = new URL(BASE_URL);
  url.hash = hash;
  return url.toString();
}

function hasErrorBoundary(text) {
  return /Application error|Something went wrong|Cannot read properties/i.test(text);
}

async function seedWikiEditPage() {
  const response = await fetch(`${API_BASE}/api/wiki/git/status`);
  if (!response.ok) {
    throw new Error(`failed to read Buddy Vault path: ${response.status}`);
  }
  const status = await response.json();
  const conceptsDir = path.join(status.vault_path, "wiki", "concepts");
  await mkdir(conceptsDir, { recursive: true });
  await writeFile(
    path.join(conceptsDir, `${SMOKE_SLUG}.md`),
    `---
type: concept
status: active
owner: smoke
schema: v1
title: Smoke Edit Page
summary: Browser smoke fixture for wiki editing
purpose:
  - learning
created_at: 2026-04-29T00:00:00Z
---

Original smoke body.
`,
    "utf8",
  );
}

function hunkSmokeBody(topLine, bottomLine) {
  const lines = [];
  for (let lineNumber = 1; lineNumber <= 80; lineNumber += 1) {
    if (lineNumber === 2) {
      lines.push(topLine);
    } else if (lineNumber === 70) {
      lines.push(bottomLine);
    } else {
      lines.push(`line ${String(lineNumber).padStart(2, "0")}`);
    }
  }
  return `${lines.join("\n")}\n`;
}

function lineSmokeBody(topInserted = false, bottomInserted = false) {
  const lines = [];
  for (let lineNumber = 1; lineNumber <= 80; lineNumber += 1) {
    lines.push(`line ${String(lineNumber).padStart(2, "0")}`);
    if (lineNumber === 2 && topInserted) {
      lines.push("line 02 inserted");
    }
    if (lineNumber === 70 && bottomInserted) {
      lines.push("line 70 inserted");
    }
  }
  return `${lines.join("\n")}\n`;
}

async function fetchJson(pathname, options) {
  const request = options
    ? {
        ...options,
        headers: {
          ...(options.body ? { "Content-Type": "application/json" } : {}),
          ...(options.headers ?? {}),
        },
      }
    : undefined;
  const response = await fetch(`${API_BASE}${pathname}`, request);
  if (!response.ok) {
    const body = await response.text();
    throw new Error(`${pathname} failed: ${response.status} ${body}`);
  }
  return response.json();
}

async function runGitHunkDiscardCheck() {
  const status = await fetchJson("/api/wiki/git/status");
  const absolutePath = path.join(status.vault_path, HUNK_SMOKE_PATH);
  await mkdir(path.dirname(absolutePath), { recursive: true });
  await writeFile(absolutePath, hunkSmokeBody("line 02", "line 70"), "utf8");

  await fetchJson("/api/wiki/git/commit", {
    method: "POST",
    body: JSON.stringify({ message: "Smoke hunk discard baseline" }),
  });

  await writeFile(
    absolutePath,
    hunkSmokeBody("line 02 changed", "line 70 changed"),
    "utf8",
  );

  const diff = await fetchJson("/api/wiki/git/diff");
  const section = diff.sections.find((candidate) => candidate.path === HUNK_SMOKE_PATH);
  if (!section || section.hunks.length < 2) {
    throw new Error(`hunk discard smoke expected at least 2 hunks for ${HUNK_SMOKE_PATH}`);
  }

  await fetchJson("/api/wiki/git/discard-hunk", {
    method: "POST",
    body: JSON.stringify({
      path: HUNK_SMOKE_PATH,
      hunk_index: 0,
      hunk_header: section.hunks[0].header,
    }),
  });

  const content = await readFile(absolutePath, "utf8");
  if (content.includes("line 02 changed")) {
    throw new Error("hunk discard smoke did not restore the selected hunk");
  }
  if (!content.includes("line 70 changed")) {
    throw new Error("hunk discard smoke removed an unrelated hunk");
  }
}

async function runGitAuditCheck() {
  const audit = await fetchJson("/api/wiki/git/audit?limit=5");
  if (!Array.isArray(audit.entries)) {
    throw new Error("git audit smoke expected an entries array");
  }

  const latest = audit.entries[0];
  if (!latest || latest.operation !== "discard-hunk") {
    throw new Error("git audit smoke expected latest operation to be discard-hunk");
  }
  if (latest.path !== HUNK_SMOKE_PATH || latest.hunk_index !== 0) {
    throw new Error("git audit smoke did not record the discarded hunk metadata");
  }
  if (!audit.entries.some((entry) => entry.operation === "commit")) {
    throw new Error("git audit smoke expected the baseline commit entry");
  }
}

async function runGitLineDiscardCheck() {
  const status = await fetchJson("/api/wiki/git/status");
  const absolutePath = path.join(status.vault_path, LINE_SMOKE_PATH);
  await mkdir(path.dirname(absolutePath), { recursive: true });
  await writeFile(absolutePath, lineSmokeBody(false, false), "utf8");

  await fetchJson("/api/wiki/git/commit", {
    method: "POST",
    body: JSON.stringify({ message: "Smoke line discard baseline" }),
  });

  await writeFile(absolutePath, lineSmokeBody(true, true), "utf8");

  const diff = await fetchJson("/api/wiki/git/diff");
  const section = diff.sections.find((candidate) => candidate.path === LINE_SMOKE_PATH);
  if (!section) {
    throw new Error(`line discard smoke expected a diff section for ${LINE_SMOKE_PATH}`);
  }
  const selected = section.hunks
    .flatMap((hunk, hunkIndex) =>
      hunk.lines.map((line, lineIndex) => ({ hunk, hunkIndex, line, lineIndex })),
    )
    .find((candidate) => candidate.line.kind === "add" && candidate.line.text === "line 02 inserted");
  if (!selected) {
    throw new Error("line discard smoke expected inserted-line metadata");
  }

  await fetchJson("/api/wiki/git/discard-line", {
    method: "POST",
    body: JSON.stringify({
      path: LINE_SMOKE_PATH,
      hunk_index: selected.hunkIndex,
      line_index: selected.lineIndex,
      hunk_header: selected.hunk.header,
      line_text: selected.line.text,
      new_line: selected.line.new_line,
    }),
  });

  const content = await readFile(absolutePath, "utf8");
  if (content.includes("line 02 inserted")) {
    throw new Error("line discard smoke did not remove the selected added line");
  }
  if (!content.includes("line 70 inserted")) {
    throw new Error("line discard smoke removed an unrelated added line");
  }

  const audit = await fetchJson("/api/wiki/git/audit?limit=5");
  const latest = audit.entries[0];
  if (!latest || latest.operation !== "discard-line" || latest.path !== LINE_SMOKE_PATH) {
    throw new Error("line discard smoke expected latest git audit operation to be discard-line");
  }
  if (latest.hunk_index !== selected.hunkIndex || latest.line_index !== selected.lineIndex) {
    throw new Error("line discard smoke did not record hunk/line metadata");
  }
}

async function runRulesFileEditCheck() {
  const targetPath = "schema/policies/naming.md";
  const before = await fetchJson(`/api/wiki/rules/file?path=${encodeURIComponent(targetPath)}`);
  if (!before.content.includes("Naming Policy")) {
    throw new Error("rules file smoke expected Naming Policy content");
  }

  const marker = "<!-- smoke-rules-file-edit -->";
  const nextContent = before.content.includes(marker)
    ? before.content
    : `${before.content.trimEnd()}\n\n${marker}\n`;
  await fetchJson("/api/wiki/rules/file", {
    method: "PUT",
    body: JSON.stringify({
      path: targetPath,
      content: nextContent,
    }),
  });

  const after = await fetchJson(`/api/wiki/rules/file?path=${encodeURIComponent(targetPath)}`);
  if (!after.content.includes(marker)) {
    throw new Error("rules file smoke did not persist the edited policy file");
  }
}

async function runWikiEditCheck(page) {
  const updatedContent = `---
type: concept
status: active
owner: smoke
schema: v1
title: Smoke Edit Page
summary: Browser smoke fixture for wiki editing
purpose:
  - learning
  - research
created_at: 2026-04-29T00:00:00Z
---

Updated smoke body from Playwright.
`;
  await page.getByRole("button", { name: "编辑此页" }).click();
  const editor = page.locator(".cm-content").first();
  await editor.waitFor({ state: "visible", timeout: 10_000 });
  await page.waitForFunction(
    () =>
      document.body.innerText.includes("Git / Lineage") &&
      document.body.innerText.includes("Vault diff"),
    null,
    { timeout: 10_000 },
  );
  await editor.click();
  await page.keyboard.press(process.platform === "darwin" ? "Meta+A" : "Control+A");
  await page.keyboard.insertText(updatedContent);
  await page.getByRole("button", { name: "保存" }).click();
  await page.waitForFunction(
    () => document.body.innerText.includes("Updated smoke body from Playwright."),
    null,
    { timeout: 10_000 },
  );
}

async function run() {
  await seedWikiEditPage();
  await runGitHunkDiscardCheck();
  await runGitAuditCheck();
  await runGitLineDiscardCheck();
  await runRulesFileEditCheck();

  const browser = await chromium.launch({ headless: HEADLESS });
  const page = await browser.newPage();
  const errors = new ErrorCollector();
  errors.attach(page);

  const results = [];
  try {
    for (const route of routes) {
      errors.drain();
      await page.goto(routeUrl(route.hash), { waitUntil: "domcontentloaded" });
      await page.waitForSelector(".ds-status-bar", { timeout: 15_000 });
      await page.waitForTimeout(500);
      const text = (await page.locator("body").innerText()).replace(/\s+/g, " ");
      for (const expected of route.mustContain) {
        if (!text.toLowerCase().includes(expected.toLowerCase())) {
          throw new Error(`${route.name} missing text: ${expected}`);
        }
      }
      if (hasErrorBoundary(text)) {
        throw new Error(`${route.name} rendered an error boundary`);
      }
      if (route.check) {
        await route.check(page);
      }
      const routeErrors = errors.drain();
      if (routeErrors.length > 0) {
        throw new Error(`${route.name} console/page errors: ${JSON.stringify(routeErrors)}`);
      }
      results.push({ route: route.name, ok: true });
    }
  } finally {
    await browser.close();
  }

  console.log(JSON.stringify({ ok: true, results }, null, 2));
}

run().catch((error) => {
  console.error(error instanceof Error ? error.stack || error.message : error);
  process.exit(1);
});
