export const SOURCE_READINESS_REQUIREMENTS = [
  {
    id: "source-evidence",
    label: "来源证据",
    description: "每条素材能回到原始出处、时间和用户确认点。",
  },
  {
    id: "dedupe",
    label: "去重策略",
    description: "同一链接、商品、歌曲或收藏不会重复淹没 Inbox。",
  },
  {
    id: "priority",
    label: "优先级信号",
    description: "能判断高信号、低信号和需要冷却的条目。",
  },
  {
    id: "cross-domain",
    label: "跨界提取",
    description: "保留原生来源，同时识别真实用途和迁移价值。",
  },
  {
    id: "archive",
    label: "归档行为",
    description: "支持可逆降噪、合并或归档，不把导入等同于永久收藏。",
  },
  {
    id: "reason-language",
    label: "原因语言",
    description: "每个建议给出人能理解的理由，而不是只给模型分数。",
  },
  {
    id: "privacy",
    label: "隐私边界",
    description: "明确本地处理、授权范围、撤销和敏感来源的默认保护。",
  },
] as const;

export type SourceReadinessRequirementId =
  (typeof SOURCE_READINESS_REQUIREMENTS)[number]["id"];

export interface SourceReadinessPlan {
  id: string;
  label: string;
  sourceKind: string;
  notes: string;
  captureOnly: boolean;
  requirements: Partial<Record<SourceReadinessRequirementId, boolean>>;
}

export interface SourceReadinessEvaluation {
  id: string;
  label: string;
  sourceKind: string;
  notes: string;
  ready: boolean;
  completed: number;
  total: number;
  missing: SourceReadinessRequirement[];
  blockers: string[];
}

export type SourceReadinessRequirement =
  (typeof SOURCE_READINESS_REQUIREMENTS)[number];

export const HIGH_VOLUME_SOURCE_PLANS: SourceReadinessPlan[] = [
  {
    id: "taobao",
    label: "淘宝 / 购物车",
    sourceKind: "购物与审美灵感",
    notes: "适合作为跨界灵感来源，但不能只把购物车整体搬进 Buddy。",
    captureOnly: true,
    requirements: {
      "source-evidence": true,
      "cross-domain": true,
      privacy: true,
    },
  },
  {
    id: "music",
    label: "音乐收藏",
    sourceKind: "情绪与社交线索",
    notes: "需要先定义情绪、关系和长期反复方向的提取边界。",
    captureOnly: true,
    requirements: {
      "source-evidence": true,
      privacy: true,
    },
  },
  {
    id: "browser-bookmarks",
    label: "浏览器收藏",
    sourceKind: "文章、链接、研究碎片",
    notes: "必须先证明去重、冷却和归档行为，不再制造另一个收藏夹。",
    captureOnly: true,
    requirements: {
      "source-evidence": true,
      dedupe: true,
      privacy: true,
    },
  },
  {
    id: "wechat",
    label: "微信现有入口",
    sourceKind: "消息、文章、URL",
    notes: "已有入口继续以 Inbox 审阅、去重、优先级和 Git 可追溯为准线。",
    captureOnly: false,
    requirements: {
      "source-evidence": true,
      dedupe: true,
      priority: true,
      "cross-domain": true,
      archive: true,
      "reason-language": true,
      privacy: true,
    },
  },
];

export function evaluateSourceReadiness(
  plan: SourceReadinessPlan,
): SourceReadinessEvaluation {
  const missing = SOURCE_READINESS_REQUIREMENTS.filter(
    (requirement) => plan.requirements[requirement.id] !== true,
  );
  const blockers = [
    plan.captureOnly ? "capture-only import is blocked" : null,
    missing.length ? `${missing.length} readiness checks missing` : null,
  ].filter(Boolean) as string[];
  return {
    id: plan.id,
    label: plan.label,
    sourceKind: plan.sourceKind,
    notes: plan.notes,
    ready: blockers.length === 0,
    completed: SOURCE_READINESS_REQUIREMENTS.length - missing.length,
    total: SOURCE_READINESS_REQUIREMENTS.length,
    missing,
    blockers,
  };
}

export function evaluateHighVolumeSourcePlans(): SourceReadinessEvaluation[] {
  return HIGH_VOLUME_SOURCE_PLANS.map(evaluateSourceReadiness);
}
