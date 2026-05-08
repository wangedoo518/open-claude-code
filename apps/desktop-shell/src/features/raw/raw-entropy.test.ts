import {
  RAW_ENTROPY_STATUS_META,
  deriveRawEntropyStatus,
} from "./raw-entropy";
import type { RawEntry } from "@/api/wiki/types";

type TestFn = () => void | Promise<void>;
interface SuiteFn {
  (name: string, fn: () => void): void;
  skip: (name: string, fn: () => void) => void;
}
interface ItFn {
  (name: string, fn: TestFn): void;
  skip: (name: string, fn: TestFn) => void;
}
interface Expect<T> {
  toBe(expected: T): void;
  toEqual(expected: unknown): void;
  toContain(expected: unknown): void;
  toBeTruthy(): void;
}
declare const describe: SuiteFn;
declare const it: ItFn;
declare const expect: <T>(actual: T) => Expect<T>;

function makeRaw(partial: Partial<RawEntry> = {}): RawEntry {
  return {
    id: 1,
    filename: "raw.md",
    source: "url",
    slug: "raw-item",
    date: "2026-05-07",
    ingested_at: "2026-05-07T00:00:00Z",
    byte_size: 1200,
    ...partial,
  };
}

describe("deriveRawEntropyStatus", () => {
  it("marks raw material with pending inbox work as crystallizable", () => {
    const result = deriveRawEntropyStatus(makeRaw({ id: 7 }), {
      pendingRawIds: new Set([7]),
    });
    expect(result.key).toBe("crystallizable");
  });

  it("marks content duplicates as duplicate candidates", () => {
    const result = deriveRawEntropyStatus(
      makeRaw({
        last_ingest_decision: {
          kind: "content_duplicate",
          matching_raw_id: 2,
          matching_url: "https://example.com/a",
        },
      }),
    );
    expect(result.key).toBe("duplicate_candidate");
  });

  it("marks approved reuse as retained", () => {
    const result = deriveRawEntropyStatus(
      makeRaw({
        last_ingest_decision: {
          kind: "reused_approved",
          reason: "already applied",
        },
      }),
    );
    expect(result.key).toBe("retained");
  });

  it("marks rejected or silent reuse as safe archive", () => {
    const rejected = deriveRawEntropyStatus(
      makeRaw({
        last_ingest_decision: {
          kind: "reused_after_reject",
          reason: "previously rejected",
        },
      }),
    );
    const silent = deriveRawEntropyStatus(
      makeRaw({
        last_ingest_decision: {
          kind: "reused_silent",
          reason: "duplicate suppressed",
        },
      }),
    );

    expect(rejected.key).toBe("safe_archive");
    expect(silent.key).toBe("safe_archive");
  });

  it("keeps unknown material observing by default", () => {
    const result = deriveRawEntropyStatus(makeRaw());
    expect(result.key).toBe("observing");
  });

  it("keeps all status metadata reversible and non-destructive", () => {
    expect(Object.keys(RAW_ENTROPY_STATUS_META)).toEqual([
      "retained",
      "observing",
      "duplicate_candidate",
      "crystallizable",
      "safe_archive",
    ]);
    for (const meta of Object.values(RAW_ENTROPY_STATUS_META)) {
      expect(meta.reversible).toBe(true);
      expect(meta.description.includes("删除")).toBe(false);
    }
  });
});
