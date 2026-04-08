/**
 * Frontend security utilities.
 *
 * Centralizes defensive helpers used by user-facing components that render
 * untrusted content such as filenames, session titles, and message previews.
 *
 * All functions in this module MUST be pure (no side effects) and safe to
 * call with arbitrary untrusted strings — including empty strings, very
 * long strings, and strings containing unusual Unicode ranges.
 */

/**
 * Unicode control characters that can visually reorder or hide text when
 * rendered in most UI contexts. These are the vectors for so-called
 * "Trojan Source" attacks and RTL-override filename spoofing.
 *
 * References:
 * - CVE-2021-42574 (Trojan Source)
 * - https://www.unicode.org/reports/tr9/ (Bidi algorithm)
 */
const UNSAFE_INVISIBLE_CHARS = [
  "\u200B", // zero-width space
  "\u200C", // zero-width non-joiner
  "\u200D", // zero-width joiner
  "\u200E", // left-to-right mark
  "\u200F", // right-to-left mark
  "\u202A", // left-to-right embedding
  "\u202B", // right-to-left embedding
  "\u202C", // pop directional formatting
  "\u202D", // left-to-right override
  "\u202E", // right-to-left override (the infamous one)
  "\u2066", // left-to-right isolate
  "\u2067", // right-to-left isolate
  "\u2068", // first strong isolate
  "\u2069", // pop directional isolate
  "\uFEFF", // zero-width no-break space / BOM
] as const;

// Precomputed regex covering all unsafe chars for one-pass stripping.
// Each codepoint is escaped into its \uXXXX form for explicitness.
const UNSAFE_CHAR_REGEX = new RegExp(
  `[${UNSAFE_INVISIBLE_CHARS.join("")}]`,
  "g",
);

/**
 * Strip invisible/directional Unicode control characters from a filename
 * so that what the user sees matches what the filename actually is.
 *
 * Without this, a file named `evil\u202Etxt.exe` would render as
 * `eviltxt.exe` in many UIs — letting an attacker disguise an executable
 * as a text file.
 *
 * Also strips NULL bytes and line separators which have no legitimate
 * place in a filename and confuse terminal output.
 *
 * @param name A (possibly untrusted) filename string
 * @returns The sanitized filename. Never returns `undefined`; returns an
 *          empty string if the input is falsy.
 *
 * @example
 *   sanitizeFilename("report.pdf")                 // → "report.pdf"
 *   sanitizeFilename("evil\u202Etxt.exe")          // → "eviltxt.exe"
 *   sanitizeFilename("中文文件.md")                  // → "中文文件.md" (CJK safe)
 *   sanitizeFilename("\u200Bhidden.sh")            // → "hidden.sh"
 *   sanitizeFilename("")                           // → ""
 *   sanitizeFilename(null as unknown as string)     // → ""
 */
export function sanitizeFilename(name: string): string {
  if (!name) return "";
  return name
    .replace(UNSAFE_CHAR_REGEX, "")
    .replace(/\0/g, "")
    .replace(/[\r\n]/g, "")
    .trim();
}

/**
 * Returns true if a string is safe to display as-is — i.e. does not
 * contain any invisible directional-control characters. Useful for
 * deciding whether to show a "sanitized" warning chip alongside a
 * filename chip.
 *
 * @example
 *   isDisplaySafe("report.pdf")          // → true
 *   isDisplaySafe("evil\u202Etxt.exe")   // → false
 */
export function isDisplaySafe(name: string): boolean {
  if (!name) return true;
  return !UNSAFE_CHAR_REGEX.test(name) && !/[\0\r\n]/.test(name);
}

/**
 * Developer-facing assertion helper used by the self-test below.
 * Kept private to this module.
 */
function assertEqual<T>(actual: T, expected: T, label: string): void {
  if (actual !== expected) {
    throw new Error(
      `security.ts self-test failed: ${label}\n  expected: ${JSON.stringify(expected)}\n  actual:   ${JSON.stringify(actual)}`,
    );
  }
}

/**
 * In-module smoke tests. Called from a browser devtools console via
 * `(await import("@/lib/security")).__runSelfTests()` or during app
 * startup in development builds.
 *
 * Not automatically invoked — must be called explicitly. This keeps the
 * module tree-shakeable and avoids side effects on import.
 */
export function __runSelfTests(): void {
  // sanitizeFilename
  assertEqual(sanitizeFilename("report.pdf"), "report.pdf", "plain ascii");
  assertEqual(
    sanitizeFilename("evil\u202Etxt.exe"),
    "eviltxt.exe",
    "RTL override removed",
  );
  assertEqual(
    sanitizeFilename("中文文件.md"),
    "中文文件.md",
    "CJK preserved",
  );
  assertEqual(
    sanitizeFilename("\u200Bhidden.sh"),
    "hidden.sh",
    "zero-width stripped",
  );
  assertEqual(sanitizeFilename(""), "", "empty input");
  assertEqual(sanitizeFilename("  trim.txt  "), "trim.txt", "whitespace trim");
  assertEqual(
    sanitizeFilename("a\0b\rc\nd.md"),
    "abcd.md",
    "NULL + CR + LF stripped",
  );
  assertEqual(
    sanitizeFilename(null as unknown as string),
    "",
    "null coerced",
  );

  // isDisplaySafe
  assertEqual(isDisplaySafe("report.pdf"), true, "safe ascii");
  assertEqual(
    isDisplaySafe("evil\u202Etxt.exe"),
    false,
    "RTL override unsafe",
  );
  assertEqual(isDisplaySafe("中文.md"), true, "CJK safe");
  assertEqual(isDisplaySafe(""), true, "empty is safe");
  assertEqual(isDisplaySafe("has\0null"), false, "NULL unsafe");

  // Hebrew text (legitimate RTL, no override) should be safe
  assertEqual(
    isDisplaySafe("שלום.txt"),
    true,
    "legitimate Hebrew preserved",
  );

  console.info("[security] self-tests passed");
}
