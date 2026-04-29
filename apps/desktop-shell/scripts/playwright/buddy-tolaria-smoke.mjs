#!/usr/bin/env node

import { chromium } from "@playwright/test";
import { ErrorCollector } from "./lib/error-collector.mjs";

const BASE_URL = process.env.BUDDY_SMOKE_URL ?? "http://127.0.0.1:5173/";
const HEADLESS = process.env.BUDDY_HEADLESS !== "false";

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
    mustContain: ["连接", "Buddy Vault / Git", "受控自动写入授权"],
  },
  {
    name: "Knowledge",
    hash: "/wiki",
    mustContain: ["知识"],
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

async function run() {
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
