import { chromium } from "@playwright/test";

const ASK_URL = process.env.CLAW_ASK_URL ?? "http://127.0.0.1:1420/#/ask";
const HEADLESS = process.env.CLAW_HEADLESS === "true";

/**
 * Launch browser, navigate to Ask page, attach error collector.
 * Returns { browser, page } - caller must close browser when done.
 */
export async function setupAskPage(errorCollector) {
  const browser = await chromium.launch({ headless: HEADLESS });
  const page = await browser.newPage();

  errorCollector.attach(page);

  await page.goto(ASK_URL);

  // Wait for composer to be ready.
  await page.waitForSelector(
    "textarea, [contenteditable='true'], .ask-composer-card",
    { timeout: 10000 },
  );

  // Small settle delay for async initialization.
  await page.waitForTimeout(500);

  return { browser, page };
}
