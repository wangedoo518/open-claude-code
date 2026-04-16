#!/usr/bin/env node
/**
 * Append-only enforcement for memory/*.jsonl files.
 *
 * Contract:
 *   - Missing canonical file → FAIL
 *   - Untracked file (new, never committed) → PASS (all content is new)
 *   - Tracked file with 0 deletions in git diff → PASS (append-only satisfied)
 *   - Tracked file with any deletions in git diff → FAIL (record edited or removed)
 *
 * This complements check-memory-jsonl.mjs (strict JSONL format).
 * Together they enforce: "valid JSON, only ever appended."
 *
 * Usage:
 *   node scripts/check-memory-append-only.mjs                    # check canonical files
 *   node scripts/check-memory-append-only.mjs file1.jsonl ...    # check specific files
 *
 * Exit 0 = all files pass. Exit 1 = at least one violation.
 */
import { existsSync } from "node:fs";
import { execSync } from "node:child_process";

const CANONICAL = ["memory/corrections.jsonl", "memory/observations.jsonl"];
const targets = process.argv.length > 2 ? process.argv.slice(2) : CANONICAL;
let failures = 0;

function isTracked(file) {
  try {
    execSync(`git ls-files --error-unmatch "${file}"`, { stdio: "pipe" });
    return true;
  } catch {
    return false;
  }
}

function getDeletionCount(file) {
  try {
    // --numstat outputs: <added>\t<deleted>\t<file>
    const out = execSync(`git diff --numstat HEAD -- "${file}"`, {
      encoding: "utf-8",
      stdio: ["pipe", "pipe", "pipe"],
    }).trim();
    if (!out) return 0; // no diff = no changes
    const parts = out.split("\t");
    return parseInt(parts[1], 10) || 0;
  } catch {
    return 0; // git error (e.g. no HEAD yet) — treat as no diff
  }
}

for (const file of targets) {
  if (!existsSync(file)) {
    console.error(`FAIL  ${file} (missing — governance files must exist)`);
    failures++;
    continue;
  }

  if (!isTracked(file)) {
    console.log(`PASS  ${file} (untracked — new file, all content is additions)`);
    continue;
  }

  const deletions = getDeletionCount(file);
  if (deletions > 0) {
    console.error(`FAIL  ${file} (${deletions} line(s) deleted — append-only violation)`);
    failures++;
  } else {
    console.log(`PASS  ${file} (tracked, 0 deletions — append-only satisfied)`);
  }
}

process.exit(failures > 0 ? 1 : 0);
