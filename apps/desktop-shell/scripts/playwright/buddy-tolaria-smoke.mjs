#!/usr/bin/env node

import { chromium } from "@playwright/test";
import { mkdir, writeFile } from "node:fs/promises";
import path from "node:path";
import { ErrorCollector } from "./lib/error-collector.mjs";

const BASE_URL = process.env.BUDDY_SMOKE_URL ?? "http://127.0.0.1:5173/";
const API_BASE = process.env.BUDDY_API_BASE ?? "http://127.0.0.1:4357";
const HEADLESS = process.env.BUDDY_HEADLESS !== "false";
const SMOKE_SLUG = "smoke-edit-page";

const routes = [
  {
    name: "Home / Pulse",
    hash: "/",
    mustContain: ["Home / Pulse", "外脑"],
  },
  {
    name: "Rules Studio",
    hash: "/rules",
    mustContain: ["Rules Studio", "Advanced YAML / CodeMirror"],
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
      "受控自动写入授权",
    ],
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
