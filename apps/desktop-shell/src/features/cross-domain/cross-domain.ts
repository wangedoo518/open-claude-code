export type SourceDomain =
  | "shopping"
  | "music"
  | "chat"
  | "article"
  | "image"
  | "file"
  | "web"
  | "unknown";

export type InferredUseDomain =
  | "aesthetic"
  | "research"
  | "decision"
  | "creative"
  | "social"
  | "healing"
  | "archive"
  | "unknown";

export interface CrossDomainInput {
  source?: string | null;
  sourceUrl?: string | null;
  title?: string | null;
  body?: string | null;
}

export interface CrossDomainInference {
  native_source: string;
  source_domain: SourceDomain;
  inferred_use_domain: InferredUseDomain;
  reason: string;
  confidence: "low" | "medium" | "high";
  ignored?: boolean;
}

export interface CrossDomainMaintainFields {
  source_domain?: SourceDomain;
  inferred_use_domain?: InferredUseDomain;
  cross_domain_reason?: string;
}

export const SOURCE_DOMAIN_LABELS: Record<SourceDomain, string> = {
  shopping: "shopping",
  music: "music",
  chat: "chat",
  article: "article",
  image: "image",
  file: "file",
  web: "web",
  unknown: "unknown",
};

export const INFERRED_USE_DOMAIN_LABELS: Record<InferredUseDomain, string> = {
  aesthetic: "aesthetic",
  research: "research",
  decision: "decision",
  creative: "creative",
  social: "social",
  healing: "healing",
  archive: "archive",
  unknown: "observing",
};

export const INFERRED_USE_DOMAIN_OPTIONS: InferredUseDomain[] = [
  "aesthetic",
  "research",
  "decision",
  "creative",
  "social",
  "healing",
  "archive",
  "unknown",
];

const SHOPPING_HOSTS = [
  "taobao.",
  "tmall.",
  "jd.",
  "1688.",
  "amazon.",
  "etsy.",
  "pinduoduo.",
  "xiaohongshu.",
];

const MUSIC_HOSTS = [
  "music.163.com",
  "spotify.",
  "music.apple.",
  "y.qq.com",
  "qqmusic.",
  "soundcloud.",
];

const AESTHETIC_KEYWORDS = [
  "aesthetic",
  "visual",
  "moodboard",
  "design",
  "palette",
  "color",
  "texture",
  "material",
  "silhouette",
  "proportion",
  "typography",
  "layout",
  "inspiration",
  "reference",
];

const DECISION_KEYWORDS = [
  "price",
  "pricing",
  "cost",
  "compare",
  "comparison",
  "decision",
  "buy",
  "purchase",
  "pros",
  "cons",
  "matrix",
];

const RESEARCH_KEYWORDS = [
  "research",
  "article",
  "paper",
  "source",
  "evidence",
  "study",
  "notes",
  "writing",
  "argument",
];

const CREATIVE_KEYWORDS = [
  "brief",
  "story",
  "creative",
  "campaign",
  "concept",
  "sketch",
  "build",
  "prototype",
];

const SOCIAL_KEYWORDS = [
  "friend",
  "friends",
  "share",
  "social",
  "conversation",
  "private",
  "memory",
];

const HEALING_KEYWORDS = [
  "healing",
  "calm",
  "comfort",
  "mood",
  "emotional",
  "recovery",
  "focus",
  "sleep",
];

function evidenceText(input: CrossDomainInput): string {
  return [input.source, input.sourceUrl, input.title, input.body]
    .filter(Boolean)
    .join(" ")
    .toLowerCase();
}

function sourceText(input: CrossDomainInput): string {
  return [input.source, input.sourceUrl].filter(Boolean).join(" ").toLowerCase();
}

function includesAny(text: string, needles: string[]): boolean {
  return needles.some((needle) => text.includes(needle));
}

function inferSourceDomain(input: CrossDomainInput): SourceDomain {
  const source = sourceText(input);
  if (includesAny(source, SHOPPING_HOSTS) || /\b(shop|shopping|store|commerce)\b/.test(source)) {
    return "shopping";
  }
  if (includesAny(source, MUSIC_HOSTS) || /\b(music|playlist|song|album)\b/.test(source)) {
    return "music";
  }
  if (/\b(wechat|chat|message|conversation)\b/.test(source)) return "chat";
  if (/\b(image|screenshot|photo|picture)\b/.test(source)) return "image";
  if (/\b(pdf|pptx|docx|file|video)\b/.test(source)) return "file";
  if (/\b(article|url|web|http)\b/.test(source)) return "article";
  return "unknown";
}

function inferUseDomain(sourceDomain: SourceDomain, text: string): {
  domain: InferredUseDomain;
  evidence: string;
  confidence: CrossDomainInference["confidence"];
} {
  if (includesAny(text, AESTHETIC_KEYWORDS)) {
    return {
      domain: "aesthetic",
      evidence: `${sourceDomain} source plus aesthetic keywords`,
      confidence: sourceDomain === "shopping" || sourceDomain === "image" ? "high" : "medium",
    };
  }
  if (includesAny(text, HEALING_KEYWORDS)) {
    return { domain: "healing", evidence: "healing and mood keywords", confidence: "medium" };
  }
  if (includesAny(text, SOCIAL_KEYWORDS)) {
    return { domain: "social", evidence: "social and memory keywords", confidence: "medium" };
  }
  if (includesAny(text, DECISION_KEYWORDS)) {
    return { domain: "decision", evidence: "decision and comparison keywords", confidence: "medium" };
  }
  if (includesAny(text, RESEARCH_KEYWORDS)) {
    return { domain: "research", evidence: "research or writing keywords", confidence: "medium" };
  }
  if (includesAny(text, CREATIVE_KEYWORDS)) {
    return { domain: "creative", evidence: "creative production keywords", confidence: "medium" };
  }
  return { domain: "unknown", evidence: "insufficient cross-domain evidence", confidence: "low" };
}

export function inferCrossDomainUse(input: CrossDomainInput): CrossDomainInference {
  const nativeSource = input.source?.trim() || "unknown";
  const sourceDomain = inferSourceDomain(input);
  const use = inferUseDomain(sourceDomain, evidenceText(input));
  return {
    native_source: nativeSource,
    source_domain: sourceDomain,
    inferred_use_domain: use.domain,
    reason: `source:${sourceDomain} -> use:${use.domain}; evidence: ${use.evidence}`,
    confidence: use.confidence,
  };
}

export function applyCrossDomainCorrection(
  inference: CrossDomainInference,
  inferredUseDomain: InferredUseDomain,
): CrossDomainInference {
  return {
    ...inference,
    inferred_use_domain: inferredUseDomain,
    reason: `source:${inference.source_domain} -> use:${inferredUseDomain}; evidence: corrected by reviewer`,
    confidence: inferredUseDomain === "unknown" ? "low" : "high",
    ignored: false,
  };
}

export function ignoreCrossDomainInference(
  inference: CrossDomainInference,
): CrossDomainInference {
  return {
    ...inference,
    ignored: true,
  };
}

export function toCrossDomainMaintainFields(
  inference: CrossDomainInference,
): CrossDomainMaintainFields {
  if (inference.ignored) return {};
  return {
    source_domain: inference.source_domain,
    inferred_use_domain: inference.inferred_use_domain,
    cross_domain_reason: inference.reason,
  };
}
