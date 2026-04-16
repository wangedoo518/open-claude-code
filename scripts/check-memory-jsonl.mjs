#!/usr/bin/env node
/**
 * Strict JSONL validator for memory/*.jsonl files.
 *
 * Contract:
 *   - Missing file → FAIL (governance files must exist)
 *   - Empty file (0 bytes or only a trailing newline) → PASS (no records yet)
 *   - Each non-empty line must be a valid JSON object
 *   - Interior blank / whitespace-only lines between records → FAIL
 *   - One trailing newline at EOF → allowed (normal text-file convention)
 *
 * Usage:
 *   node scripts/check-memory-jsonl.mjs              # validate canonical memory files
 *   node scripts/check-memory-jsonl.mjs file1 file2   # validate specific files
 *
 * Exit 0 = all files valid. Exit 1 = at least one failure.
 */
import { readFileSync, existsSync } from "node:fs";

const CANONICAL = ["memory/corrections.jsonl", "memory/observations.jsonl"];
const targets = process.argv.length > 2 ? process.argv.slice(2) : CANONICAL;
let failures = 0;

for (const file of targets) {
  // Missing file is a governance violation — these files must exist.
  if (!existsSync(file)) {
    console.error(`FAIL  ${file} (missing — governance files must exist)`);
    failures++;
    continue;
  }

  const content = readFileSync(file, "utf-8");

  // Strip at most one trailing newline (normal text-file convention).
  const stripped = content.endsWith("\n") ? content.slice(0, -1) : content;

  // Truly empty file (0 records) is valid.
  if (stripped.length === 0) {
    console.log(`PASS  ${file} (empty — 0 records)`);
    continue;
  }

  const lines = stripped.split("\n");
  let ok = true;

  for (let i = 0; i < lines.length; i++) {
    const line = lines[i];
    const lineNum = i + 1;

    // Interior blank / whitespace-only line → strict JSONL violation.
    if (line.trim().length === 0) {
      console.error(`FAIL  ${file}:${lineNum}: blank line (strict JSONL forbids interior blank lines)`);
      ok = false;
      failures++;
      continue;
    }

    try {
      JSON.parse(line);
    } catch (e) {
      console.error(`FAIL  ${file}:${lineNum}: ${e.message}`);
      ok = false;
      failures++;
    }
  }

  if (ok) console.log(`PASS  ${file} (${lines.length} valid records)`);
}

process.exit(failures > 0 ? 1 : 0);
