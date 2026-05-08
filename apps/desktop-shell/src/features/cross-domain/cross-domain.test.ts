import {
  applyCrossDomainCorrection,
  inferCrossDomainUse,
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
