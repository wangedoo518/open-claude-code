/**
 * Captures pageerror events and console.error messages from a Playwright page.
 * Used by regression tests to verify zero runtime errors.
 */
export class ErrorCollector {
  constructor() {
    this.errors = [];
  }

  /**
   * Attach error listeners to a Playwright page.
   * Call this before navigation.
   */
  attach(page) {
    page.on("pageerror", (error) => {
      this.errors.push({
        type: "pageerror",
        message: error.message,
        stack: error.stack,
        timestamp: Date.now(),
      });
    });

    page.on("console", (msg) => {
      if (msg.type() === "error") {
        const text = msg.text();

        // Filter known browser/dev noise.
        if (text.includes("Failed to load resource")) return;
        if (text.includes("Download the React DevTools")) return;

        this.errors.push({
          type: "console_error",
          message: text,
          timestamp: Date.now(),
        });
      }
    });
  }

  /**
   * Drain all collected errors and reset.
   * Returns the errors that occurred since last drain.
   */
  drain() {
    const drained = [...this.errors];
    this.errors = [];
    return drained;
  }
}
