#!/usr/bin/env node

import { spawn } from "node:child_process";
import { mkdtemp, mkdir, rm, writeFile } from "node:fs/promises";
import { existsSync, writeSync } from "node:fs";
import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";

const scriptDir = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(scriptDir, "..");
const rustDir = path.join(repoRoot, "rust");
const shellDir = path.join(repoRoot, "apps", "desktop-shell");
const targetDir = path.join(rustDir, "target", "debug");
const serverBin = path.join(
  targetDir,
  process.platform === "win32" ? "desktop-server.exe" : "desktop-server",
);
const cargoBin = process.platform === "win32" ? "cargo.exe" : "cargo";
const npmExec = makeNodePackageManagerExec("npm");
const npxExec = makeNodePackageManagerExec("npx");

const args = new Set(process.argv.slice(2));
const keepTemp = args.has("--keep-temp");
const skipBuild = args.has("--skip-build");
const skipBrowser = args.has("--skip-browser");

const children = new Set();
const usedPorts = new Set();
let tempRoot = null;

function makeNodePackageManagerExec(kind) {
  if (process.platform !== "win32") {
    return { command: kind, prefixArgs: [] };
  }
  const appData = process.env.APPDATA || path.join(os.homedir(), "AppData", "Roaming");
  const cli = path.join(appData, "npm", "node_modules", "npm", "bin", `${kind}-cli.js`);
  if (existsSync(cli)) {
    return { command: process.execPath, prefixArgs: [cli] };
  }
  return { command: `${kind}.cmd`, prefixArgs: [] };
}

function needsShell(command) {
  return process.platform === "win32" && /\.(cmd|bat)$/i.test(command);
}

function quoteCmdArg(value) {
  const s = String(value);
  if (/^[A-Za-z0-9_./:=\\-]+$/.test(s)) {
    return s;
  }
  return `"${s.replace(/"/g, '""')}"`;
}

function normalizeSpawn(command, commandArgs) {
  if (!needsShell(command)) {
    return { command, commandArgs };
  }
  const comspec = process.env.ComSpec || "cmd.exe";
  const commandLine = [command, ...commandArgs].map(quoteCmdArg).join(" ");
  return {
    command: comspec,
    commandArgs: ["/d", "/s", "/c", commandLine],
  };
}

function log(message) {
  writeSync(2, `[phase5-smoke] ${message}\n`);
}

function fail(message) {
  throw new Error(message);
}

function run(command, commandArgs, options = {}) {
  return new Promise((resolve, reject) => {
    log(`$ ${command} ${commandArgs.join(" ")}`);
    const normalized = normalizeSpawn(command, commandArgs);
    const child = spawn(normalized.command, normalized.commandArgs, {
      cwd: options.cwd ?? repoRoot,
      env: options.env ?? process.env,
      stdio: options.stdio ?? "inherit",
      shell: false,
    });
    child.on("error", reject);
    child.on("exit", (code, signal) => {
      if (code === 0) {
        resolve();
      } else {
        reject(
          new Error(
            `${command} exited with ${code ?? `signal ${signal ?? "unknown"}`}`,
          ),
        );
      }
    });
  });
}

function runExec(exec, commandArgs, options = {}) {
  return run(exec.command, [...exec.prefixArgs, ...commandArgs], options);
}

function capture(command, commandArgs, options = {}) {
  return new Promise((resolve, reject) => {
    const normalized = normalizeSpawn(command, commandArgs);
    const child = spawn(normalized.command, normalized.commandArgs, {
      cwd: options.cwd ?? repoRoot,
      env: options.env ?? process.env,
      stdio: ["ignore", "pipe", "pipe"],
      shell: false,
    });
    let stdout = "";
    let stderr = "";
    child.stdout.on("data", (chunk) => {
      stdout += chunk.toString();
    });
    child.stderr.on("data", (chunk) => {
      stderr += chunk.toString();
    });
    child.on("error", reject);
    child.on("exit", (code, signal) => {
      if (code === 0) {
        resolve({ stdout, stderr });
      } else {
        const error = new Error(
          `${command} ${commandArgs.join(" ")} exited with ${
            code ?? `signal ${signal ?? "unknown"}`
          }\n${stderr || stdout}`,
        );
        error.stdout = stdout;
        error.stderr = stderr;
        reject(error);
      }
    });
  });
}

function captureExec(exec, commandArgs, options = {}) {
  return capture(exec.command, [...exec.prefixArgs, ...commandArgs], options);
}

function spawnManaged(name, command, commandArgs, options = {}) {
  log(`start ${name}: ${command} ${commandArgs.join(" ")}`);
  const normalized = normalizeSpawn(command, commandArgs);
  const child = spawn(normalized.command, normalized.commandArgs, {
    cwd: options.cwd ?? repoRoot,
    env: options.env ?? process.env,
    stdio: ["ignore", "pipe", "pipe"],
    shell: false,
  });
  children.add(child);
  const tail = [];
  const pushTail = (prefix, chunk) => {
    for (const line of chunk.toString().split(/\r?\n/)) {
      if (!line.trim()) continue;
      const rendered = `[${name}] ${prefix}${line}`;
      tail.push(rendered);
      if (tail.length > 80) tail.shift();
      if (options.echo) process.stdout.write(`${rendered}\n`);
    }
  };
  child.stdout.on("data", (chunk) => pushTail("", chunk));
  child.stderr.on("data", (chunk) => pushTail("ERR ", chunk));
  child.on("exit", () => children.delete(child));
  child.on("error", (error) => {
    children.delete(child);
    tail.push(`[${name}] spawn error: ${error.message}`);
  });
  child.tail = tail;
  return child;
}

function spawnManagedExec(name, exec, commandArgs, options = {}) {
  return spawnManaged(name, exec.command, [...exec.prefixArgs, ...commandArgs], options);
}

async function getFreePort() {
  let port = 0;
  do {
    port = 49152 + Math.floor(Math.random() * 12000);
  } while (usedPorts.has(port));
  usedPorts.add(port);
  return port;
}

async function waitForUrl(url, options = {}) {
  const deadline = Date.now() + (options.timeoutMs ?? 120_000);
  let lastError = null;
  while (Date.now() < deadline) {
    try {
      const response = await fetch(url, { method: options.method ?? "GET" });
      if (response.ok) return response;
      lastError = new Error(`${url} returned HTTP ${response.status}`);
    } catch (error) {
      lastError = error;
    }
    await delay(options.intervalMs ?? 500);
  }
  fail(
    `Timed out waiting for ${url}${
      lastError ? `: ${lastError.message}` : ""
    }`,
  );
}

async function jsonRequest(url, options = {}) {
  const response = await fetch(url, {
    method: options.method ?? "GET",
    headers: {
      Accept: "application/json",
      ...(options.body ? { "Content-Type": "application/json" } : {}),
      ...(options.headers ?? {}),
    },
    body: options.body ? JSON.stringify(options.body) : undefined,
  });
  const text = await response.text();
  let payload = null;
  try {
    payload = text ? JSON.parse(text) : null;
  } catch {
    payload = text;
  }
  if (!response.ok) {
    fail(`${url} returned HTTP ${response.status}: ${text}`);
  }
  return payload;
}

function delay(ms) {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

async function seedWiki(root) {
  const conceptsDir = path.join(root, "wiki", "concepts");
  await mkdir(conceptsDir, { recursive: true });
  await mkdir(path.join(root, ".claw"), { recursive: true });
  const alpha = "Alpha planning note ".repeat(42).trim();
  const beta = "Beta operations note ".repeat(42).trim();
  const body = `---
type: concept
status: draft
owner: maintainer
schema: v1
title: Phase 5 Source
summary: Mixed page used by the Phase 5 power-tools smoke test.
created_at: 2026-04-25T00:00:00Z
confidence: 0.91
---

# Phase 5 Source

This page intentionally has multiple sections so /breakdown has stable split targets.

## Alpha

${alpha}

## Beta

${beta}
`;
  await writeFile(path.join(conceptsDir, "phase5-source.md"), body, "utf8");
}

function assertArray(value, label) {
  if (!Array.isArray(value)) {
    fail(`${label} must be an array`);
  }
}

async function runHttpSmoke(apiBase) {
  log("HTTP smoke: /cleanup preview");
  const cleanup = await jsonRequest(`${apiBase}/api/wiki/cleanup?apply=false`, {
    method: "POST",
  });
  if (cleanup.applied !== false) fail("/cleanup preview must not apply");
  assertArray(cleanup.issues, "cleanup.issues");
  assertArray(cleanup.cleanup_proposals, "cleanup.cleanup_proposals");

  log("HTTP smoke: /breakdown preview");
  const preview = await jsonRequest(`${apiBase}/api/wiki/breakdown`, {
    method: "POST",
    body: { slug: "phase5-source", apply: false, max_targets: 4 },
  });
  if (preview.applied !== false) fail("/breakdown preview must not apply");
  assertArray(preview.targets, "breakdown.preview.targets");
  if (preview.targets.length < 2) {
    fail(`/breakdown preview expected at least 2 targets, got ${preview.targets.length}`);
  }

  log("HTTP smoke: /breakdown apply");
  const applied = await jsonRequest(`${apiBase}/api/wiki/breakdown`, {
    method: "POST",
    body: { slug: "phase5-source", apply: true, max_targets: 4 },
  });
  if (applied.applied !== true) fail("/breakdown apply must report applied=true");
  assertArray(applied.written_paths, "breakdown.apply.written_paths");
  if (applied.written_paths.length < 2) {
    fail(`/breakdown apply expected at least 2 written paths, got ${applied.written_paths.length}`);
  }
  const targetSlug = applied.targets?.[0]?.slug;
  if (!targetSlug) fail("/breakdown apply returned no target slug");
  const target = await jsonRequest(
    `${apiBase}/api/wiki/pages/${encodeURIComponent(targetSlug)}`,
  );
  if (!target.body?.includes("Split from [Phase 5 Source]")) {
    fail(`split target ${targetSlug} did not retain source backlink`);
  }
}

async function runPlaywright(commandArgs, options = {}) {
  const baseArgs = [
    "--yes",
    "--package",
    "@playwright/cli",
    "playwright-cli",
    `-s=${options.session}`,
  ];
  if (options.raw) baseArgs.push("--raw");
  return captureExec(npxExec, [...baseArgs, ...commandArgs], {
    cwd: repoRoot,
    env: process.env,
  });
}

async function openBrowserWithRetry(session, url) {
  try {
    await runPlaywright(["open", url], { session });
  } catch (error) {
    const message = `${error.stderr ?? ""}\n${error.stdout ?? ""}\n${error.message}`;
    if (
      message.includes("Executable doesn't exist") ||
      message.toLowerCase().includes("browser")
    ) {
      log("Playwright browser missing; installing chromium through playwright-cli");
      await runPlaywright(["install-browser", "chromium"], { session });
      await runPlaywright(["open", url], { session });
      return;
    }
    throw error;
  }
}

async function expectBrowserText(session, url, expected) {
  log(`browser smoke: ${url}`);
  await openBrowserWithRetry(session, url);
  const deadline = Date.now() + 45_000;
  let text = "";
  while (Date.now() < deadline) {
    const result = await runPlaywright(["eval", "() => document.body.innerText"], {
      session,
      raw: true,
    });
    text = result.stdout.trim();
    const normalized = text.toLowerCase();
    if (expected.every((needle) => normalized.includes(needle.toLowerCase()))) return;
    await delay(750);
  }
  fail(
    `Timed out waiting for browser text ${JSON.stringify(expected)} at ${url}\n` +
      text.slice(0, 2000),
  );
}

async function runBrowserSmoke(webBase) {
  if (skipBrowser) {
    log("browser smoke skipped by --skip-browser");
    return;
  }
  await captureExec(npxExec, ["--version"]);
  const session = `phase5-power-tools-${process.pid}`;
  try {
    await expectBrowserText(session, `${webBase}/#/viewer`, [
      "Web viewer",
      "Recent wiki pages",
    ]);
    await expectBrowserText(session, `${webBase}/#/viewer/wiki/phase5-source`, [
      "Phase 5 Source",
      "Alpha",
    ]);
    await expectBrowserText(session, `${webBase}/#/viewer/graph`, [
      "Graph entrypoint",
      "Raw sources",
    ]);
  } finally {
    try {
      await runPlaywright(["close"], { session });
    } catch {
      // Best-effort cleanup only.
    }
  }
}

async function shutdownServer(apiBase, token, child) {
  try {
    await fetch(`${apiBase}/internal/shutdown`, {
      method: "POST",
      headers: { "x-shutdown-token": token },
    });
  } catch {
    // Fall through to process kill.
  }
  await waitForExitOrKill(child, 5_000);
}

async function waitForExitOrKill(child, timeoutMs) {
  if (!child || child.exitCode !== null) return;
  const exited = new Promise((resolve) => child.once("exit", resolve));
  const timedOut = delay(timeoutMs).then(() => "timeout");
  if ((await Promise.race([exited, timedOut])) === "timeout") {
    child.kill();
  }
}

async function cleanup() {
  for (const child of Array.from(children)) {
    await waitForExitOrKill(child, 1_000);
  }
  if (tempRoot && !keepTemp) {
    await rm(tempRoot, { recursive: true, force: true });
  } else if (tempRoot) {
    log(`kept temp root: ${tempRoot}`);
  }
}

async function main() {
  tempRoot = await mkdtemp(path.join(os.tmpdir(), "clawwiki-phase5-"));
  const wikiRoot = path.join(tempRoot, "wiki-root");
  await mkdir(wikiRoot, { recursive: true });
  await seedWiki(wikiRoot);

  const apiPort = await getFreePort();
  const webPort = await getFreePort();
  const apiBase = `http://127.0.0.1:${apiPort}`;
  const webBase = `http://127.0.0.1:${webPort}`;
  const shutdownToken = `phase5-smoke-${process.pid}`;

  await run(cargoBin, ["build", "-p", "desktop-server"], { cwd: rustDir });
  if (!existsSync(serverBin)) fail(`desktop-server binary not found at ${serverBin}`);

  const server = spawnManaged("desktop-server", serverBin, [], {
    cwd: repoRoot,
    env: {
      ...process.env,
      CLAWWIKI_HOME: wikiRoot,
      CLAW_CONFIG_HOME: path.join(tempRoot, ".claw"),
      OPEN_CLAUDE_CODE_DESKTOP_ADDR: `127.0.0.1:${apiPort}`,
      OCL_SHUTDOWN_TOKEN: shutdownToken,
    },
  });
  await waitForUrl(`${apiBase}/healthz`, { timeoutMs: 120_000 });
  await runHttpSmoke(apiBase);

  if (!skipBuild) {
    await runExec(npmExec, ["run", "build"], {
      cwd: shellDir,
      env: {
        ...process.env,
        VITE_DESKTOP_API_BASE: apiBase,
      },
    });
  }

  const preview = spawnManagedExec(
    "vite-preview",
    npmExec,
    ["run", "preview", "--", "--host", "127.0.0.1", "--port", String(webPort), "--strictPort"],
    {
      cwd: shellDir,
      env: process.env,
    },
  );
  await waitForUrl(webBase, { timeoutMs: 60_000 });
  await runBrowserSmoke(webBase);

  await waitForExitOrKill(preview, 100);
  await shutdownServer(apiBase, shutdownToken, server);
  log("PASS phase5 power-tools smoke");
}

main()
  .catch(async (error) => {
    writeSync(2, `\n[phase5-smoke] FAIL ${error.stack ?? error.message}\n`);
    process.exitCode = 1;
  })
  .finally(async () => {
    await cleanup();
    process.exit(process.exitCode ?? 0);
  });
