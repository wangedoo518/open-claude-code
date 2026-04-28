function composerLocator(page) {
  return page.locator("textarea, [contenteditable='true']").first();
}

/**
 * Send a message via the composer.
 * Assumes composer is interactive.
 */
export async function sendMessage(page, text) {
  const composer = composerLocator(page);
  await composer.click();
  await composer.fill(text);
  await composer.press("Enter");
}

/**
 * Press Escape to interrupt the current turn.
 */
export async function pressEsc(page) {
  await page.keyboard.press("Escape");
}

/**
 * Wait for the turn to reach a terminal state.
 * Terminal states: idle / ok / error (based on .ask-state-dot[data-tone]).
 */
export async function waitForTerminalPhase(page, timeoutMs = 5000) {
  await page.waitForFunction(
    () => {
      const dot = document.querySelector(".ask-state-dot");
      const tone = dot?.getAttribute("data-tone");
      return ["idle", "ok", "error"].includes(tone);
    },
    { timeout: timeoutMs },
  );
}

/**
 * Check if composer is currently editable.
 */
export async function isComposerInteractive(page) {
  const composer = composerLocator(page);
  return composer.isEditable().catch(() => false);
}

/**
 * Get current state-dot tone for diagnostic purposes.
 */
export async function getCurrentTone(page) {
  return page
    .locator(".ask-state-dot")
    .first()
    .getAttribute("data-tone")
    .catch(() => null);
}

export async function startNewConversation(page) {
  const clicked = await page.evaluate(() => {
    const button = document.querySelector(".ask-history-new");
    if (!(button instanceof HTMLElement)) return false;
    button.click();
    return true;
  });

  if (!clicked) {
    const button = page.locator(".ask-history-new").first();
    await button.click({ timeout: 3000, force: true });
  }

  await page.waitForTimeout(400);
}

export async function waitForComposerReady(page, timeoutMs = 10000) {
  await page.waitForSelector(
    "textarea, [contenteditable='true'], .ask-composer-card",
    { timeout: timeoutMs },
  );
  await page.waitForTimeout(400);
}

export async function getAskText(page) {
  return page.locator("body").innerText({ timeout: 5000 });
}

export function hasErrorBoundary(text) {
  return /Cannot read properties|Application error|error boundary|Something went wrong|undefined \(reading 'messages'\)/i.test(
    text,
  );
}
