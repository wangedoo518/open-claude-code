/**
 * Slice E28.2 — categorizeAction pure-fn tests.
 *
 * Ambient-vitest contract.
 */

import { categorizeAction } from "./action-category";

declare const describe: (n: string, fn: () => void) => void;
declare const it: (n: string, fn: () => void) => void;
declare const expect: <T>(actual: T) => {
  toBe(expected: T): void;
  toEqual(expected: unknown): void;
};

describe("categorizeAction", () => {
  it("recognises /inbox as inbox category", () => {
    expect(categorizeAction("/inbox").id).toBe("inbox");
  });

  it("recognises /connections#git as vault category", () => {
    expect(categorizeAction("/connections#git").id).toBe("vault");
  });

  it("recognises /wiki and /wiki?... as wiki category", () => {
    expect(categorizeAction("/wiki").id).toBe("wiki");
    expect(categorizeAction("/wiki?source=missing").id).toBe("wiki");
  });

  it("recognises /raw?add=... as ingest category", () => {
    expect(categorizeAction("/raw?add=url").id).toBe("ingest");
    expect(categorizeAction("/raw?add=file").id).toBe("ingest");
  });

  it("recognises /wechat as wechat category", () => {
    expect(categorizeAction("/wechat").id).toBe("wechat");
  });

  it("recognises /ask as express category", () => {
    expect(categorizeAction("/ask").id).toBe("express");
  });

  it("recognises /drafts as express category", () => {
    expect(categorizeAction("/drafts").id).toBe("express");
  });

  it("recognises /rules as patrol category", () => {
    expect(categorizeAction("/rules#validation").id).toBe("patrol");
  });

  it("falls back to default for unrecognised hrefs", () => {
    const cat = categorizeAction("/something-new");
    expect(cat.id).toBe("default");
  });
});
