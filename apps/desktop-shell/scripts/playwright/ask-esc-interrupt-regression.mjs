#!/usr/bin/env node

import { ErrorCollector } from "./lib/error-collector.mjs";
import { setupAskPage } from "./lib/playwright-setup.mjs";
import {
  getAskText,
  hasErrorBoundary,
  isComposerInteractive,
  pressEsc,
  sendMessage,
  startNewConversation,
} from "./lib/ask-helpers.mjs";

const INTERRUPT_SETTLE_MS = Number(process.env.CLAW_ESC_SETTLE_MS ?? 4200);
const FAIL_ON_REGRESSION = process.env.CLAW_FAIL_ON_REGRESSION !== "false";

const phases = [
  {
    name: "sending",
    prompt: "Search today's weather and summarize it in one sentence.",
    wait: async (page) => {
      await page.waitForTimeout(120);
      return { reached: true, elapsedMs: 120 };
    },
  },
  {
    name: "thinking",
    prompt: "Explain why sleep matters in three concise sentences. Think first, then answer.",
    wait: async (page) => {
      await page.waitForTimeout(900);
      return { reached: true, elapsedMs: 900 };
    },
  },
  {
    name: "tool_running",
    prompt: "Search today's real-time Guangzhou weather and summarize it in one sentence.",
    wait: async (page) => waitForToolGroup(page, 15000),
  },
  {
    name: "tool_waiting",
    prompt: "Search for the latest DeepSeek V4 release information, read relevant pages, and summarize it in two sentences.",
    wait: async (page) => {
      const result = await waitForToolCompleted(page, 30000);
      await page.waitForTimeout(300);
      return result;
    },
  },
  {
    name: "streaming",
    prompt: "Write a 1200-character Chinese essay about how external knowledge bases help personal knowledge management.",
    wait: async (page) => {
      await page.waitForTimeout(3000);
      return { reached: true, elapsedMs: 3000 };
    },
  },
];

async function waitForTextState(page, predicate, timeoutMs) {
  const startedAt = Date.now();
  while (Date.now() - startedAt < timeoutMs) {
    const text = await getAskText(page);
    if (await predicate(text)) {
      return { reached: true, elapsedMs: Date.now() - startedAt };
    }
    await page.waitForTimeout(250);
  }
  return { reached: false, elapsedMs: Date.now() - startedAt };
}

async function waitForToolGroup(page, timeoutMs) {
  const startedAt = Date.now();
  try {
    await page.locator(".ask-tool-group").first().waitFor({
      state: "visible",
      timeout: timeoutMs,
    });
    return { reached: true, elapsedMs: Date.now() - startedAt };
  } catch {
    return { reached: false, elapsedMs: Date.now() - startedAt };
  }
}

async function waitForToolCompleted(page, timeoutMs) {
  const cssResult = await waitForTextState(
    page,
    () => page.locator(".ask-tool-row--completed").count().then((count) => count > 0),
    1,
  ).catch(() => ({ reached: false, elapsedMs: 0 }));

  if (cssResult.reached) return cssResult;

  return waitForTextState(
    page,
    (text) => /completed|done|tool/i.test(text),
    timeoutMs,
  );
}

function summarizeRows(rows) {
  const summary = {
    total: rows.length,
    passed: rows.filter((row) => row.passed).length,
    failed: rows.filter((row) => !row.passed).length,
    byPhase: {},
  };

  for (const row of rows) {
    summary.byPhase[row.phase] ??= { total: 0, passed: 0, failed: 0 };
    summary.byPhase[row.phase].total += 1;
    if (row.passed) {
      summary.byPhase[row.phase].passed += 1;
    } else {
      summary.byPhase[row.phase].failed += 1;
    }
  }

  return summary;
}

async function runInterruptCase(page, errorCollector, phase, iteration) {
  errorCollector.drain();

  await startNewConversation(page);
  await sendMessage(page, `${phase.prompt} [Esc regression ${phase.name} ${iteration}]`);

  const waitResult = await phase.wait(page);
  const beforeEscText = await getAskText(page);

  await pressEsc(page);
  await page.waitForTimeout(INTERRUPT_SETTLE_MS);

  const afterEscText = await getAskText(page);
  const errors = errorCollector.drain();
  const composerInteractive = await isComposerInteractive(page);
  const boundary = hasErrorBoundary(beforeEscText) || hasErrorBoundary(afterEscText);
  const stopped = /stopped|cancelled by user|completed|done|interrupted/i.test(afterEscText);

  const passed = errors.length === 0 && !boundary && composerInteractive;

  return {
    kind: "interrupt",
    phase: phase.name,
    iteration,
    passed,
    waitReached: waitResult.reached,
    waitMs: waitResult.elapsedMs,
    boundary,
    stopped,
    composerInteractive,
    errors,
  };
}

async function runSendAfterInterruptCase(page, errorCollector, iteration) {
  errorCollector.drain();

  await startNewConversation(page);
  await sendMessage(page, `Search today's weather, summarize it, then allow interrupt test ${iteration}.`);
  await page.waitForTimeout(1400);
  await pressEsc(page);
  await page.waitForTimeout(300);

  const followup = `Post-interrupt follow-up ${iteration}: reply with one short sentence.`;
  await sendMessage(page, followup);
  await page.waitForTimeout(12000);

  const text = await getAskText(page);
  const errors = errorCollector.drain();
  const boundary = hasErrorBoundary(text);
  const composerInteractive = await isComposerInteractive(page);
  const newMessagePresent = text.includes(followup);
  const assistantPresent = text.includes("ASSISTANT");
  const passed = errors.length === 0
    && !boundary
    && composerInteractive
    && newMessagePresent
    && assistantPresent;

  return {
    kind: "send_after_interrupt",
    phase: "send_after_interrupt",
    iteration,
    passed,
    boundary,
    composerInteractive,
    newMessagePresent,
    assistantPresent,
    errors,
  };
}

async function main() {
  const errorCollector = new ErrorCollector();
  const { browser, page } = await setupAskPage(errorCollector);
  const rows = [];

  try {
    for (const phase of phases) {
      for (let iteration = 1; iteration <= 5; iteration += 1) {
        const row = await runInterruptCase(page, errorCollector, phase, iteration);
        rows.push(row);
        console.log(
          `${row.passed ? "PASS" : "FAIL"} ${row.phase} #${iteration} `
          + `wait=${row.waitReached ? "hit" : "miss"} `
          + `errors=${row.errors.length} boundary=${row.boundary}`,
        );
      }
    }

    for (let iteration = 1; iteration <= 3; iteration += 1) {
      const row = await runSendAfterInterruptCase(page, errorCollector, iteration);
      rows.push(row);
      console.log(
        `${row.passed ? "PASS" : "FAIL"} send_after_interrupt #${iteration} `
        + `errors=${row.errors.length} boundary=${row.boundary}`,
      );
    }
  } finally {
    await browser.close();
  }

  const summary = summarizeRows(rows);
  console.log("\n=== Summary ===");
  console.log(JSON.stringify(summary, null, 2));

  const failedRows = rows.filter((row) => !row.passed);
  if (failedRows.length > 0) {
    console.log("\n=== Failed cases ===");
    console.log(JSON.stringify(failedRows, null, 2));
  }

  console.log(
    summary.failed === 0
      ? `\nOK ${summary.passed}/${summary.total} tests passed`
      : `\nFAIL ${summary.passed}/${summary.total} tests passed`,
  );

  if (summary.failed > 0 && FAIL_ON_REGRESSION) {
    process.exitCode = 1;
  }
}

main().catch((error) => {
  console.error(error);
  process.exitCode = 1;
});
