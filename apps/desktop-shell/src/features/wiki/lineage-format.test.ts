/**
 * P1 sprint — unit tests for the `lineage-format` pure-function
 * helpers owned by Worker B (tone / icon / display-title / upstream
 * / downstream rendering / relative-time formatter / combined-apply
 * collapse helpers).
 *
 * Authored in the same contract-form pattern used by
 * `queue-intelligence.test.ts` (Q1), `candidate-scoring.test.ts` (Q2),
 * `combined-proposal-rules.test.ts` (W3), and `health-state.test.ts`
 * (M5):
 *
 *   • vitest is not wired into `apps/desktop-shell` yet — there is
 *     no `vitest` devDependency and no `vitest.config.ts`. Importing
 *     from `"vitest"` would break `tsc --noEmit` on developer
 *     machines that haven't installed the package.
 *
 *   • To keep the file green under the project-wide `tsc --noEmit`
 *     gate while still documenting the locked behavioural contract,
 *     we declare local ambient globals that mirror the subset of
 *     vitest's chai-flavoured API the test bodies need. When a
 *     later sprint wires in vitest + config, the ambient block
 *     becomes a harmless duplicate declaration that can be deleted.
 *
 *   • Worker A has not yet shipped the canonical `LineageEvent` /
 *     `LineageRef` / `LineageEventType` exports in `@/lib/tauri`.
 *     Worker B's `lineage-format.ts` currently re-declares the shape
 *     as `LineageEventLike` (and exports concrete `LineageEventType`
 *     + `LineageRef` unions of its own). These tests import the
 *     Worker B aliases so the contract is exercised end-to-end
 *     today; Main's integrator will flip both the helper module and
 *     this test file to `import type { LineageEvent, LineageRef,
 *     LineageEventType } from "@/lib/tauri";` once Worker A lands.
 *
 * Coverage matrix (what this file asserts about the helpers):
 *
 *   toneFor — 4-way classifier:
 *     raw_written                   → source (info-blue family)
 *     wechat_message_received       → source
 *     url_ingested                  → source
 *     inbox_appended                → neutral
 *     proposal_generated            → neutral
 *     wiki_page_applied             → applied (success-green)
 *     combined_wiki_page_applied    → applied
 *     inbox_rejected                → warning
 *     (every `LineageEventType` is exhaustively covered so adding a
 *     variant without classifying it fails loudly)
 *
 *   iconFor / iconNameFor alias:
 *     every `LineageEventType` yields a non-null `LucideIcon`
 *     combined-apply and single-apply share the same family
 *       (`GitMerge` vs `CheckCircle2` — distinct icons so applied
 *       rows differentiate bundle vs single — we only check both
 *       are truthy).
 *     `iconNameFor` alias echoes the same component reference.
 *
 *   displayTitleFor:
 *     `display_title` populated → returns it verbatim
 *     `display_title` blank     → falls back to a zh-CN label per
 *       event_type; every branch returns a non-empty string that
 *       contains at least one CJK character
 *
 *   formatUpstream / formatDownstream (both alias formatRefs):
 *     empty                        → "—"
 *     1 raw                        → "raw #00001"
 *     2 raws                       → "raw #00123 · raw #00456"
 *     3 refs (max)                 → no "+N 更多" suffix
 *     6 inboxes (combined apply)   → head 3 visible + "+3 更多"
 *                                    (does NOT fan out all six ids)
 *     url_source                   → shortened to hostname/pathname
 *     wiki_page with title         → title wins over slug
 *
 *   formatRelativeTime:
 *     delta < 0  (clock skew)      → "刚刚"
 *     delta < 60 s / = 0           → "刚刚"
 *     60 s ≤ delta < 60 min        → "N 分钟前"
 *     60 min ≤ delta < 24 h        → "N 小时前"
 *     24 h ≤ delta                 → "N 天前"
 *     monotonic: larger delta produces a bucket index ≥ smaller
 *
 *   isCombinedApply / combinedApplyInboxCount:
 *     only fires on "combined_wiki_page_applied"
 *     inbox count falls back to total upstream when no inbox refs
 */

import {
  combinedApplyInboxCount,
  displayTitleFor,
  formatDownstream,
  formatRefs,
  formatRelativeTime,
  formatUpstream,
  iconFor,
  iconNameFor,
  isCombinedApply,
  refLabel,
  TONE_CLASSES,
  toneFor,
  type LineageEventLike,
  type LineageEventType,
  type LineageRef,
  type LineageTone,
} from "./lineage-format";

// ── Local vitest ambient globals (drop once vitest is installed) ───

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
  toBeGreaterThan(expected: number): void;
  toBeGreaterThanOrEqual(expected: number): void;
  toBeLessThan(expected: number): void;
  toBeLessThanOrEqual(expected: number): void;
  toBeDefined(): void;
  toBeUndefined(): void;
  toBeTruthy(): void;
  toBeFalsy(): void;
  toContain(expected: unknown): void;
  toMatch(expected: RegExp | string): void;
  toHaveLength(expected: number): void;
  not: Expect<T>;
}
declare const describe: SuiteFn;
declare const it: ItFn;
declare const expect: <T>(actual: T) => Expect<T>;

// ── Fixture helpers ────────────────────────────────────────────────

const FIXED_NOW = 1_700_000_000_000;

const SECOND_MS = 1_000;
const MINUTE_MS = 60 * SECOND_MS;
const HOUR_MS = 60 * MINUTE_MS;
const DAY_MS = 24 * HOUR_MS;

/**
 * Minimal `LineageEventLike` factory with every optional slot zeroed
 * or blanked; callers override only the slots relevant to the rule
 * under test.
 */
function makeEvent(
  partial: Partial<LineageEventLike> = {},
): LineageEventLike {
  return {
    event_id: "evt-1",
    event_type: "raw_written",
    timestamp_ms: FIXED_NOW,
    upstream: [],
    downstream: [],
    display_title: "",
    metadata: {},
    ...partial,
  };
}

/** Shorthand constructors for the five `LineageRef` kinds. */
const rawRef = (id: number): LineageRef => ({ kind: "raw", id });
const inboxRef = (id: number): LineageRef => ({ kind: "inbox", id });
const wikiRef = (slug: string, title?: string | null): LineageRef => ({
  kind: "wiki_page",
  slug,
  title: title ?? null,
});
const urlRef = (canonical: string): LineageRef => ({
  kind: "url_source",
  canonical,
});
const wechatRef = (event_key: string): LineageRef => ({
  kind: "wechat_message",
  event_key,
});

/**
 * Exhaustive list of every `LineageEventType` — used by the coverage
 * sweeps so forgetting to classify a newly-added kind causes an
 * obvious test failure rather than silent drift.
 */
const ALL_EVENT_TYPES: LineageEventType[] = [
  "raw_written",
  "inbox_appended",
  "proposal_generated",
  "wiki_page_applied",
  "combined_wiki_page_applied",
  "inbox_rejected",
  "wechat_message_received",
  "url_ingested",
];

// ── toneFor — 4-way classifier ─────────────────────────────────────

describe("toneFor — 色调分类", () => {
  it("raw_written → source", () => {
    expect(toneFor("raw_written")).toBe("source");
  });

  it("wechat_message_received → source", () => {
    expect(toneFor("wechat_message_received")).toBe("source");
  });

  it("url_ingested → source", () => {
    expect(toneFor("url_ingested")).toBe("source");
  });

  it("inbox_appended → neutral", () => {
    expect(toneFor("inbox_appended")).toBe("neutral");
  });

  it("proposal_generated → neutral", () => {
    expect(toneFor("proposal_generated")).toBe("neutral");
  });

  it("wiki_page_applied → applied", () => {
    expect(toneFor("wiki_page_applied")).toBe("applied");
  });

  it("combined_wiki_page_applied → applied", () => {
    expect(toneFor("combined_wiki_page_applied")).toBe("applied");
  });

  it("inbox_rejected → warning", () => {
    expect(toneFor("inbox_rejected")).toBe("warning");
  });

  it("exhaustive: 每个 LineageEventType 都落入 4 色之一", () => {
    // Guard against a new variant being added to the union without
    // being classified — catches regressions the moment a ninth kind
    // ships in Worker A.
    const allowed: LineageTone[] = ["source", "neutral", "applied", "warning"];
    const allowedSet = new Set<string>(allowed);
    for (const t of ALL_EVENT_TYPES) {
      expect(allowedSet.has(toneFor(t))).toBe(true);
    }
  });

  it("TONE_CLASSES 覆盖 4 个 tone", () => {
    // Sanity: each tone has a non-empty `pill` + `text` class string
    // so UI rows never render with missing Tailwind classes.
    const keys: LineageTone[] = ["source", "neutral", "applied", "warning"];
    for (const k of keys) {
      expect(TONE_CLASSES[k]).toBeDefined();
      expect(TONE_CLASSES[k].pill.length).toBeGreaterThan(0);
      expect(TONE_CLASSES[k].text.length).toBeGreaterThan(0);
    }
  });
});

// ── iconFor / iconNameFor alias ────────────────────────────────────

describe("iconFor — 每个 event_type 返回非空 LucideIcon", () => {
  it("每个 event_type 返回一个组件引用", () => {
    for (const t of ALL_EVENT_TYPES) {
      const icon = iconFor(t);
      // LucideIcon is a function-like React component; the concrete
      // identity test is "truthy + typeof function-or-object".
      expect(icon).toBeTruthy();
      const kind = typeof icon;
      expect(kind === "function" || kind === "object").toBe(true);
    }
  });

  it("iconNameFor 是 iconFor 的别名", () => {
    // The brief used `iconNameFor`; the module exports it as an
    // alias pointing at `iconFor`. Any divergence between the two
    // would silently drop one call-site on the floor.
    for (const t of ALL_EVENT_TYPES) {
      expect(iconNameFor(t)).toBe(iconFor(t));
    }
  });

  it("applied vs combined_applied 使用不同 icon（区分单 vs 合并）", () => {
    // Both are in the "applied" tone, but the glyph must differ so
    // the timeline communicates "single page apply" vs "bundle
    // merge apply" at a glance.
    expect(iconFor("wiki_page_applied")).not.toBe(
      iconFor("combined_wiki_page_applied"),
    );
  });
});

// ── displayTitleFor ────────────────────────────────────────────────

describe("displayTitleFor — 标题生成", () => {
  it("显式 display_title 非空 → 直接返回", () => {
    const event = makeEvent({
      event_type: "combined_wiki_page_applied",
      display_title: "已合并 6 条素材到 example-domain",
    });
    expect(displayTitleFor(event)).toBe("已合并 6 条素材到 example-domain");
  });

  it("display_title 为空 → 回退到 zh-CN 标签（每类都非空 + 含 CJK）", () => {
    for (const t of ALL_EVENT_TYPES) {
      const event = makeEvent({ event_type: t, display_title: "" });
      const title = displayTitleFor(event);
      expect(typeof title).toBe("string");
      expect(title.length).toBeGreaterThan(0);
      expect(title).toMatch(/[\u4e00-\u9fff]/);
    }
  });

  it("display_title 只含空白 → 回退而非原样返回", () => {
    const event = makeEvent({
      event_type: "raw_written",
      display_title: "   ",
    });
    // Whitespace-only input is treated as empty; fallback label
    // should fire so the UI never renders an empty row.
    const title = displayTitleFor(event);
    expect(title.length).toBeGreaterThan(0);
    expect(title).not.toBe("   ");
  });
});

// ── refLabel ───────────────────────────────────────────────────────

describe("refLabel — 单 ref 短格式渲染", () => {
  it("raw → 'raw #' + 5 位 padded id", () => {
    // padStart(5, "0") means id 7 renders as "raw #00007".
    expect(refLabel(rawRef(7))).toBe("raw #00007");
    expect(refLabel(rawRef(12345))).toBe("raw #12345");
  });

  it("inbox → 'inbox #<id>'（无 padding）", () => {
    expect(refLabel(inboxRef(42))).toBe("inbox #42");
  });

  it("wiki_page 有 title → title 优先于 slug", () => {
    expect(refLabel(wikiRef("example-domain", "Example Domain"))).toBe(
      "Example Domain",
    );
  });

  it("wiki_page 无 title 或 title 为空 → 返回 slug", () => {
    expect(refLabel(wikiRef("example-domain", null))).toBe("example-domain");
    expect(refLabel(wikiRef("example-domain", ""))).toBe("example-domain");
  });

  it("url_source 合法 URL → hostname + path", () => {
    expect(refLabel(urlRef("https://example.com/page"))).toBe(
      "example.com/page",
    );
  });

  it("url_source 根路径 → 只 hostname（不挂 '/'）", () => {
    expect(refLabel(urlRef("https://example.com/"))).toBe("example.com");
  });

  it("url_source 非法 URL → 返回原字符串（不崩）", () => {
    expect(refLabel(urlRef("not-a-url"))).toBe("not-a-url");
  });

  it("wechat_message → 'WeChat (<前 8 位>…)'", () => {
    const label = refLabel(wechatRef("abcdef1234567890"));
    expect(label).toContain("WeChat");
    expect(label).toContain("abcdef12");
    // Must NOT include the full 16-char key.
    expect(label.includes("abcdef1234567890")).toBe(false);
  });
});

// ── formatRefs / formatUpstream / formatDownstream ─────────────────

describe("formatRefs — 列表格式化", () => {
  it("空数组 → '—'", () => {
    expect(formatRefs([])).toBe("—");
    expect(formatUpstream([])).toBe("—");
    expect(formatDownstream([])).toBe("—");
  });

  it("1 个 ref → 单 label", () => {
    expect(formatRefs([rawRef(1)])).toBe("raw #00001");
  });

  it("2 个 ref → '·' 连接", () => {
    expect(formatRefs([rawRef(123), rawRef(456)])).toBe(
      "raw #00123 · raw #00456",
    );
  });

  it("恰好 3 个 ref（max 边界）→ 无 '更多' 后缀", () => {
    const text = formatRefs([rawRef(1), rawRef(2), rawRef(3)], 3);
    expect(text).toBe("raw #00001 · raw #00002 · raw #00003");
    expect(text.includes("更多")).toBe(false);
  });

  it("6 inbox（combined apply）→ 头 3 + '+3 更多'（紧凑渲染）", () => {
    // The combined-proposal path can bundle up to 6 inboxes. The
    // upstream summary MUST NOT fan out all six ids into the string
    // or the timeline row wraps awkwardly. Instead, head-3 + count
    // tail is the contract.
    const refs: LineageRef[] = Array.from({ length: 6 }, (_, i) =>
      inboxRef(i + 1),
    );
    const text = formatRefs(refs);
    expect(text).toContain("inbox #1");
    expect(text).toContain("inbox #2");
    expect(text).toContain("inbox #3");
    expect(text).toContain("+3 更多");
    // None of the 4..6 ids should appear — they are collapsed into
    // the tail count.
    expect(text.includes("inbox #4")).toBe(false);
    expect(text.includes("inbox #5")).toBe(false);
    expect(text.includes("inbox #6")).toBe(false);
  });

  it("custom max=1 → head 1 + '+N 更多'", () => {
    const refs: LineageRef[] = [rawRef(1), rawRef(2), rawRef(3)];
    const text = formatRefs(refs, 1);
    expect(text).toContain("raw #00001");
    expect(text).toContain("+2 更多");
  });

  it("formatUpstream / formatDownstream 与 formatRefs 同步", () => {
    // The two aliases must echo `formatRefs` output byte-for-byte so
    // callers can swap based on intent without changing the render.
    const refs: LineageRef[] = [rawRef(1), inboxRef(5)];
    expect(formatUpstream(refs)).toBe(formatRefs(refs));
    expect(formatDownstream(refs)).toBe(formatRefs(refs));
  });
});

// ── formatRelativeTime — zh-CN 相对时间 ────────────────────────────

describe("formatRelativeTime — zh-CN 相对时间", () => {
  it("delta = 0 → '刚刚'", () => {
    expect(formatRelativeTime(FIXED_NOW, FIXED_NOW)).toBe("刚刚");
  });

  it("delta < 60s → '刚刚'", () => {
    expect(formatRelativeTime(FIXED_NOW - 5 * SECOND_MS, FIXED_NOW)).toBe(
      "刚刚",
    );
  });

  it("future (clock skew, delta < 0) → '刚刚'", () => {
    // Defensive: if backend ts is ahead of renderer clock the
    // formatter should still return a sane label rather than
    // "-1 分钟前".
    expect(formatRelativeTime(FIXED_NOW + 2 * SECOND_MS, FIXED_NOW)).toBe(
      "刚刚",
    );
  });

  it("分钟 bucket: 5 分钟前", () => {
    expect(formatRelativeTime(FIXED_NOW - 5 * MINUTE_MS, FIXED_NOW)).toMatch(
      /^5\s*分钟前$/,
    );
  });

  it("分钟 bucket: 59 分钟前 (上边界)", () => {
    expect(formatRelativeTime(FIXED_NOW - 59 * MINUTE_MS, FIXED_NOW)).toMatch(
      /^59\s*分钟前$/,
    );
  });

  it("小时 bucket: 1 小时前 (下边界)", () => {
    expect(formatRelativeTime(FIXED_NOW - 1 * HOUR_MS, FIXED_NOW)).toMatch(
      /^1\s*小时前$/,
    );
  });

  it("小时 bucket: 23 小时前 (上边界)", () => {
    expect(formatRelativeTime(FIXED_NOW - 23 * HOUR_MS, FIXED_NOW)).toMatch(
      /^23\s*小时前$/,
    );
  });

  it("天 bucket: 1 天前 (下边界)", () => {
    expect(formatRelativeTime(FIXED_NOW - 1 * DAY_MS, FIXED_NOW)).toMatch(
      /^1\s*天前$/,
    );
  });

  it("天 bucket: 7 天前", () => {
    expect(formatRelativeTime(FIXED_NOW - 7 * DAY_MS, FIXED_NOW)).toMatch(
      /^7\s*天前$/,
    );
  });

  it("时序保证: delta 递增 → bucket 索引不下降", () => {
    // Sort a spread of deltas ascending and walk the bucket index
    // across `刚刚` → 分钟 → 小时 → 天. The bucket for each
    // successive delta must be >= the previous one — a small but
    // powerful guard against someone accidentally flipping a
    // threshold the wrong way.
    const bucket = (label: string): number => {
      if (label === "刚刚") return 0;
      if (label.endsWith("分钟前")) return 1;
      if (label.endsWith("小时前")) return 2;
      if (label.endsWith("天前")) return 3;
      return 99;
    };
    const deltas = [
      10 * SECOND_MS, // 刚刚
      2 * MINUTE_MS, // 分钟
      45 * MINUTE_MS, // 分钟
      3 * HOUR_MS, // 小时
      22 * HOUR_MS, // 小时
      2 * DAY_MS, // 天
      30 * DAY_MS, // 天
    ];
    let prev = -1;
    for (const d of deltas) {
      const label = formatRelativeTime(FIXED_NOW - d, FIXED_NOW);
      const b = bucket(label);
      expect(b).toBeGreaterThanOrEqual(prev);
      prev = b;
    }
  });
});

// ── isCombinedApply / combinedApplyInboxCount ──────────────────────

describe("isCombinedApply — combined-apply 判定", () => {
  it("combined_wiki_page_applied → true", () => {
    expect(
      isCombinedApply(makeEvent({ event_type: "combined_wiki_page_applied" })),
    ).toBe(true);
  });

  it("其他 event_type → false（穷举）", () => {
    for (const t of ALL_EVENT_TYPES) {
      if (t === "combined_wiki_page_applied") continue;
      expect(isCombinedApply(makeEvent({ event_type: t }))).toBe(false);
    }
  });
});

describe("combinedApplyInboxCount — inbox 计数回退", () => {
  it("upstream 含 6 inbox → 6", () => {
    const refs: LineageRef[] = Array.from({ length: 6 }, (_, i) =>
      inboxRef(i + 1),
    );
    const event = makeEvent({
      event_type: "combined_wiki_page_applied",
      upstream: refs,
    });
    expect(combinedApplyInboxCount(event)).toBe(6);
  });

  it("upstream 混合 → 只计 inbox 类型", () => {
    const event = makeEvent({
      event_type: "combined_wiki_page_applied",
      upstream: [inboxRef(1), inboxRef(2), rawRef(9), wikiRef("p")],
    });
    expect(combinedApplyInboxCount(event)).toBe(2);
  });

  it("upstream 没 inbox → 回退到 upstream.length", () => {
    // When no refs are inbox-kind (e.g. a direct-applied URL) the
    // helper returns the full upstream length so the UI still has
    // a plausible count to surface.
    const event = makeEvent({
      event_type: "combined_wiki_page_applied",
      upstream: [rawRef(1), rawRef(2), rawRef(3)],
    });
    expect(combinedApplyInboxCount(event)).toBe(3);
  });

  it("upstream 空 → 0", () => {
    const event = makeEvent({
      event_type: "combined_wiki_page_applied",
      upstream: [],
    });
    expect(combinedApplyInboxCount(event)).toBe(0);
  });
});
