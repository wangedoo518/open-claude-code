import { SLASH_COMMANDS } from "./SlashCommandPalette";

declare const describe: (name: string, fn: () => void) => void;
declare const it: (name: string, fn: () => void) => void;
declare const expect: <T>(actual: T) => {
  toBe(expected: T): void;
  toContain(expected: unknown): void;
  toBeTruthy(): void;
};

describe("SlashCommandPalette cross-domain prompts", () => {
  it("exposes source-vs-use reflection templates for Ask", () => {
    const crossDomain = SLASH_COMMANDS.find((cmd) => cmd.name === "/cross-domain");
    const brief = SLASH_COMMANDS.find((cmd) => cmd.name === "/brief");

    expect(crossDomain?.action).toBe("prompt");
    expect(crossDomain?.prompt).toContain("source -> likely use");
    expect(brief?.prompt).toContain("brief");
    expect(brief?.prompt).toContain("优先级");
  });

  it("exposes entropy reflection templates as slash commands", () => {
    const names = SLASH_COMMANDS.map((cmd) => cmd.name);

    expect(names).toContain("/continue");
    expect(names).toContain("/safe-archive");
    expect(names).toContain("/recurring");
    expect(names).toContain("/why-priority");
    expect(names).toContain("/theme-brief");
  });
});
