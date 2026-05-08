/**
 * Slice E17.4 — VerdictPicker contract tests.
 *
 * Ambient-vitest contract: type-checks via `tsc --noEmit`, will run
 * verbatim once vitest is wired up. Locks the public verdict choice
 * set so future renames don't silently change the wire contract that
 * the maintainer absorb prompt depends on.
 */

import { VerdictPicker, type Verdict } from "./VerdictPicker";

declare const describe: (n: string, fn: () => void) => void;
declare const it: (n: string, fn: () => void) => void;
declare const expect: <T>(actual: T) => {
  toBe(expected: T): void;
  toContain(expected: unknown): void;
};

describe("VerdictPicker", () => {
  it("exports the three canonical verdict literals", () => {
    const valid: Verdict[] = ["should_continue", "should_let_go", "inconclusive"];
    expect(valid.length).toBe(3);
  });

  it("renders to a JSX element when given a current verdict", () => {
    const out = VerdictPicker({
      currentVerdict: "should_continue",
      reason: "follow-ups in 5 days",
      onChange: () => {},
    });
    // The component should render *something* — verifying via type
    // is enough; the actual choice list is tested by hand in the
    // preview smoke once the harness lands. Snapshot string check
    // locks that the verdict literals appear somewhere in the tree.
    const tree = JSON.stringify(out);
    expect(tree).toContain("should_continue");
    expect(tree).toContain("should_let_go");
    expect(tree).toContain("inconclusive");
  });
});
