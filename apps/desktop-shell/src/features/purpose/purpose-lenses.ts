export const PURPOSE_LENSES = [
  {
    id: "writing",
    label: "Writing",
    zhLabel: "写作",
    output: "文章 / 随笔 / 帖子 / 报告",
  },
  {
    id: "building",
    label: "Building",
    zhLabel: "构建",
    output: "产品 / 代码 / 方案 / 项目决策",
  },
  {
    id: "operating",
    label: "Operating",
    zhLabel: "运营",
    output: "系统 / 流程 / KPI / 组织决策",
  },
  {
    id: "learning",
    label: "Learning",
    zhLabel: "学习",
    output: "掌握 / 表达 / 学习卡片",
  },
  {
    id: "personal",
    label: "Personal",
    zhLabel: "个人",
    output: "反思 / 习惯 / 生活决策",
  },
  {
    id: "research",
    label: "Research",
    zhLabel: "研究",
    output: "研究地图 / 假设 / 分析 memo",
  },
] as const;

export type PurposeLensId = (typeof PURPOSE_LENSES)[number]["id"];

export const PURPOSE_LENS_IDS = PURPOSE_LENSES.map((lens) => lens.id);

const PURPOSE_LABEL_BY_ID = new Map<string, string>(
  PURPOSE_LENSES.map((lens) => [lens.id, lens.zhLabel] as const),
);

export function purposeLensLabel(id: string): string {
  return PURPOSE_LABEL_BY_ID.get(id) ?? id;
}

export function normalizePurposeLens(raw: string): string {
  return raw.trim().toLocaleLowerCase();
}

export function isValidPurposeLens(raw: string): boolean {
  const lens = normalizePurposeLens(raw);
  return /^[a-z0-9-]+$/.test(lens);
}
