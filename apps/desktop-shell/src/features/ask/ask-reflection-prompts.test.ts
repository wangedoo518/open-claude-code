import { ASK_REFLECTION_PROMPTS } from "./ask-reflection-prompts";

declare const describe: (name: string, fn: () => void) => void;
declare const it: (name: string, fn: () => void) => void;
declare const expect: <T>(actual: T) => {
  toBe(expected: T): void;
  toBeGreaterThanOrEqual(expected: number): void;
  toContain(expected: unknown): void;
};

describe("ASK_REFLECTION_PROMPTS", () => {
  it("covers continue, archive, recurrence, priority, and brief workflows", () => {
    const ids = ASK_REFLECTION_PROMPTS.map((prompt) => prompt.id);
    expect(ids).toContain("continue");
    expect(ids).toContain("safe_archive");
    expect(ids).toContain("recurring_themes");
    expect(ids).toContain("priority_reason");
    expect(ids).toContain("brief");
  });

  it("requires source citations and observing language for weak evidence", () => {
    const combined = ASK_REFLECTION_PROMPTS.map((prompt) => prompt.prompt).join("\n");
    expect(combined).toContain("引用来源");
    expect(combined).toContain("观察中");
    expect(ASK_REFLECTION_PROMPTS.length).toBeGreaterThanOrEqual(5);
  });
});
