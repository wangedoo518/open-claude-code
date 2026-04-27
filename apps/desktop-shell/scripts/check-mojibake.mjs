#!/usr/bin/env node

import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const appRoot = path.resolve(__dirname, "..");
const srcRoot = path.join(appRoot, "src");

const TARGET_EXTENSIONS = new Set([".ts", ".tsx", ".css"]);
const dictionaryPath = path.join(__dirname, "mojibake-dictionary.json");

const MOJIBAKE_DICTIONARY = fs.existsSync(dictionaryPath)
  ? JSON.parse(fs.readFileSync(dictionaryPath, "utf8"))
  : {};

const SUSPICIOUS_LEADING = [
  "鍒",
  "鍔",
  "鍕",
  "鍖",
  "鍗",
  "鍘",
  "鍙",
  "鍚",
  "鍛",
  "鍝",
  "鍞",
  "鍟",
  "鍠",
  "鏂",
  "鏃",
  "鏄",
  "鏅",
  "鏈",
  "鏉",
  "鏊",
  "鏋",
  "鏌",
  "鏍",
  "鏎",
  "鏏",
  "鏐",
  "鏒",
  "鏔",
  "鎵",
  "鎶",
  "鎷",
  "鎹",
  "鎺",
  "鎻",
  "鎽",
  "鎾",
  "鎿",
  "姝",
  "姞",
  "姠",
  "娆",
  "娉",
  "娣",
  "娥",
  "婀",
  "婁",
  "婃",
  "婆",
  "銆",
  "銇",
  "銈",
  "銉",
  "璇",
  "璐",
  "璞",
  "瀛",
  "瀵",
  "瀹",
];

const suspiciousLeadingPattern = new RegExp(
  `[${SUSPICIOUS_LEADING.join("")}][\\u4E00-\\u9FFF\\u3000-\\u303F\\uE000-\\uF8FF\\uFF00-\\uFFEF]`,
  "u",
);

const MOJIBAKE_PATTERNS = [
  ...Object.keys(MOJIBAKE_DICTIONARY).map((key) => ({
    label: `known mojibake dictionary entry`,
    pattern: new RegExp(escapeRegExp(key), "u"),
    priority: 100,
  })),
  { label: "common mojibake: 鏂板", pattern: /鏂板/u, priority: 80 },
  { label: "common mojibake: 鍑嗗", pattern: /鍑嗗/u, priority: 80 },
  { label: "common mojibake: 鎵ц", pattern: /鎵ц/u, priority: 80 },
  { label: "common mojibake punctuation: 鈥", pattern: /鈥/u, priority: 80 },
  { label: "common mojibake punctuation: 锛", pattern: /锛/u, priority: 80 },
  { label: "unicode replacement character", pattern: /\uFFFD/u, priority: 80 },
  // UTF-8 decoded as Latin-1/Windows-1252 often leaves Chinese text as
  // fragments such as "æ‰§è¡Œ". Keep this broad but limited to source text.
  { label: "UTF-8 decoded as Latin-1", pattern: /æ[\u0080-\uFFFF]{2}/u, priority: 80 },
  {
    label: "UTF-8 decoded as GBK/CP936 suspicious leading sequence",
    pattern: suspiciousLeadingPattern,
    priority: 10,
  },
];

function escapeRegExp(value) {
  return value.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
}

function toGlobalPattern(pattern) {
  return new RegExp(pattern.source, pattern.flags.includes("g") ? pattern.flags : `${pattern.flags}g`);
}

function walk(dir) {
  const entries = fs.readdirSync(dir, { withFileTypes: true });
  const files = [];

  for (const entry of entries) {
    const absolute = path.join(dir, entry.name);
    if (entry.isDirectory()) {
      files.push(...walk(absolute));
      continue;
    }

    if (entry.isFile() && TARGET_EXTENSIONS.has(path.extname(entry.name))) {
      files.push(absolute);
    }
  }

  return files;
}

function locationFor(content, index) {
  const before = content.slice(0, index);
  const lines = before.split(/\r\n|\r|\n/);
  const line = lines.length;
  const column = lines[lines.length - 1].length + 1;
  return { line, column };
}

function lineAt(content, lineNumber) {
  return content.split(/\r\n|\r|\n/)[lineNumber - 1]?.trim() ?? "";
}

function contextAround(content, index, length) {
  const start = Math.max(0, index - 30);
  const end = Math.min(content.length, index + length + 30);
  return content.slice(start, end).replace(/\s+/g, " ").trim();
}

if (!fs.existsSync(srcRoot)) {
  console.error(`[mojibake] Source directory not found: ${srcRoot}`);
  process.exit(1);
}

const rawFindings = [];

for (const file of walk(srcRoot)) {
  const content = fs.readFileSync(file, "utf8");

  for (const { label, pattern, priority } of MOJIBAKE_PATTERNS) {
    const globalPattern = toGlobalPattern(pattern);
    for (const match of content.matchAll(globalPattern)) {
      if (match.index == null) continue;

      const { line, column } = locationFor(content, match.index);
      const matched = match[0];
      const likelyMeant = MOJIBAKE_DICTIONARY[matched] ?? null;
      rawFindings.push({
        file: path.relative(appRoot, file),
        absoluteFile: file,
        start: match.index,
        end: match.index + matched.length,
        priority,
        line,
        column,
        label,
        matched,
        likelyMeant,
        snippet: lineAt(content, line),
        context: contextAround(content, match.index, matched.length),
      });
    }
  }
}

const findings = [];
const occupied = new Map();

for (const finding of rawFindings.sort((a, b) => {
  if (a.absoluteFile !== b.absoluteFile) return a.absoluteFile.localeCompare(b.absoluteFile);
  if (a.start !== b.start) return a.start - b.start;
  if (a.priority !== b.priority) return b.priority - a.priority;
  return (b.end - b.start) - (a.end - a.start);
})) {
  const ranges = occupied.get(finding.absoluteFile) ?? [];
  const overlaps = ranges.some((range) => finding.start < range.end && finding.end > range.start);
  if (overlaps) continue;
  ranges.push({ start: finding.start, end: finding.end });
  occupied.set(finding.absoluteFile, ranges);
  findings.push(finding);
}

if (findings.length > 0) {
  console.error(
    `[mojibake] Found ${findings.length} possible mojibake sequence(s).`,
  );
  console.error(
    "[mojibake] Fix the source text or update check-mojibake.mjs if this is a deliberate false positive.",
  );

  for (const finding of findings) {
    console.error(
      `\n${finding.file}:${finding.line}:${finding.column} ${finding.label}`,
    );
    console.error(`  matched: ${finding.matched}`);
    if (finding.likelyMeant) {
      console.error(`  likely_meant: ${finding.likelyMeant}`);
      console.error(`  suggested_fix: replace with "${finding.likelyMeant}"`);
    }
    console.error(`  context: ${finding.context}`);
    console.error(`  ${finding.snippet}`);
  }

  process.exit(1);
}

console.log("[mojibake] No mojibake sequences found.");
