import {
  applyCrossDomainCorrection,
  computeAcceptRate,
  type FeedbackEvent,
  inferCrossDomainUse,
  shouldDegradeInference,
  toCrossDomainMaintainFields,
} from "./cross-domain";

declare const describe: (name: string, fn: () => void) => void;
declare const it: (name: string, fn: () => void) => void;
declare const expect: <T>(actual: T) => {
  toBe(expected: T): void;
  toEqual(expected: unknown): void;
  toContain(expected: unknown): void;
};

describe("cross-domain inference", () => {
  it("separates shopping source from aesthetic use when the evidence says design inspiration", () => {
    const result = inferCrossDomainUse({
      source: "url",
      sourceUrl: "https://item.taobao.com/item.htm?id=1",
      title: "sofa color palette design reference",
      body: "material texture, silhouette, proportion, visual inspiration",
    });

    expect(result.source_domain).toBe("shopping");
    expect(result.inferred_use_domain).toBe("aesthetic");
    expect(result.native_source).toBe("url");
    expect(result.reason).toContain("shopping");
  });

  it("treats music collections as healing or social only when there is matching evidence", () => {
    const result = inferCrossDomainUse({
      source: "music",
      title: "late night healing playlist",
      body: "calm, comfort, private mood, emotional recovery",
    });

    expect(result.source_domain).toBe("music");
    expect(result.inferred_use_domain).toBe("healing");
  });

  it("keeps weak shopping evidence in unknown use instead of forcing a creative label", () => {
    const result = inferCrossDomainUse({
      source: "url",
      sourceUrl: "https://detail.tmall.com/item.htm?id=2",
      title: "chair",
      body: "saved for later",
    });

    expect(result.source_domain).toBe("shopping");
    expect(result.inferred_use_domain).toBe("unknown");
    expect(result.reason).toContain("insufficient");
  });

  it("lets correction change the inferred use without mutating the native source or source domain", () => {
    const original = inferCrossDomainUse({
      source: "url",
      sourceUrl: "https://item.taobao.com/item.htm?id=3",
      title: "lamp reference",
      body: "pricing, compare, decision matrix",
    });

    const corrected = applyCrossDomainCorrection(original, "aesthetic");

    expect(corrected.source_domain).toBe(original.source_domain);
    expect(corrected.native_source).toBe(original.native_source);
    expect(corrected.inferred_use_domain).toBe("aesthetic");
  });

  it("serializes only the maintain fields that should be written to wiki frontmatter", () => {
    const result = inferCrossDomainUse({
      source: "image",
      title: "visual moodboard",
      body: "layout, color, typography",
    });

    expect(toCrossDomainMaintainFields(result)).toEqual({
      source_domain: "image",
      inferred_use_domain: "aesthetic",
      cross_domain_reason:
        "source:image -> use:aesthetic; evidence: image source plus aesthetic keywords",
    });
  });
});

// ── E13.3: rolling accept-rate + degrade gate ─────────────────────
describe("computeAcceptRate", () => {
  const NOW = 1_700_000_000_000;

  it("returns 1.0 when every event in the window is accept", () => {
    const events: FeedbackEvent[] = [
      { decision: "accept", source_domain: "shopping", timestamp_ms: NOW - 1000 },
      { decision: "accept", source_domain: "shopping", timestamp_ms: NOW - 2000 },
    ];
    expect(computeAcceptRate(events, "shopping", 30, NOW)).toBe(1);
  });

  it("returns 0.5 with one accept + one correct", () => {
    const events: FeedbackEvent[] = [
      { decision: "accept", source_domain: "shopping", timestamp_ms: NOW - 1000 },
      { decision: "correct", source_domain: "shopping", timestamp_ms: NOW - 2000 },
    ];
    expect(computeAcceptRate(events, "shopping", 30, NOW)).toBe(0.5);
  });

  it("ignores events older than the window", () => {
    const events: FeedbackEvent[] = [
      // 31 days old: out of 30-day window
      { decision: "accept", source_domain: "shopping", timestamp_ms: NOW - 31 * 86400000 },
      // fresh: in the window
      { decision: "correct", source_domain: "shopping", timestamp_ms: NOW - 1000 },
    ];
    // Only the correct event counts → 0/1
    expect(computeAcceptRate(events, "shopping", 30, NOW)).toBe(0);
  });

  it("ignores events from other source domains", () => {
    const events: FeedbackEvent[] = [
      { decision: "correct", source_domain: "music", timestamp_ms: NOW - 1000 },
      { decision: "accept", source_domain: "shopping", timestamp_ms: NOW - 1000 },
    ];
    expect(computeAcceptRate(events, "shopping", 30, NOW)).toBe(1);
  });

  it("returns 0 when there are no events for the domain", () => {
    expect(computeAcceptRate([], "shopping", 30, NOW)).toBe(0);
  });
});

describe("shouldDegradeInference", () => {
  const NOW = 1_700_000_000_000;

  function feedbackEvents(count: number, decision: string): FeedbackEvent[] {
    return Array.from({ length: count }, (_, i) => ({
      decision,
      source_domain: "shopping",
      timestamp_ms: NOW - i * 1000,
    }));
  }

  it("does not degrade when sample size < 20", () => {
    const events = feedbackEvents(19, "ignore");
    expect(shouldDegradeInference(events, "shopping", NOW)).toBe(false);
  });

  it("degrades when sample ≥ 20 and accept_rate < 0.5", () => {
    const events: FeedbackEvent[] = [
      ...feedbackEvents(10, "accept"),
      ...feedbackEvents(15, "correct"),
    ];
    expect(shouldDegradeInference(events, "shopping", NOW)).toBe(true);
  });

  it("does not degrade when sample ≥ 20 but accept_rate ≥ 0.5", () => {
    const events = feedbackEvents(20, "accept");
    expect(shouldDegradeInference(events, "shopping", NOW)).toBe(false);
  });

  it("treats per-source-domain isolation: shopping degrade does not bleed into music", () => {
    const events: FeedbackEvent[] = [
      ...feedbackEvents(20, "correct"),
      ...Array.from({ length: 5 }, (_, i) => ({
        decision: "accept",
        source_domain: "music",
        timestamp_ms: NOW - i * 1000,
      })),
    ];
    expect(shouldDegradeInference(events, "shopping", NOW)).toBe(true);
    expect(shouldDegradeInference(events, "music", NOW)).toBe(false);
  });
});
