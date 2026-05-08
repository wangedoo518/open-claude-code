export type AskReflectionPromptId =
  | "continue"
  | "safe_archive"
  | "recurring_themes"
  | "priority_reason"
  | "brief";

export interface AskReflectionPrompt {
  id: AskReflectionPromptId;
  slashName: string;
  description: string;
  prompt: string;
}

export const ASK_REFLECTION_PROMPTS: AskReflectionPrompt[] = [
  {
    id: "continue",
    slashName: "/continue",
    description: "判断哪些主题值得继续推进",
    prompt:
      "哪些内容值得继续推进？请按主题分组，引用来源，说明优先级理由；证据弱的只标为观察中。",
  },
  {
    id: "safe_archive",
    slashName: "/safe-archive",
    description: "找出可以冷却或归档的素材",
    prompt:
      "哪些素材可以安全冷却或归档？请引用来源，说明为什么低信号/重复/过期，并保留可反悔的建议。",
  },
  {
    id: "recurring_themes",
    slashName: "/recurring",
    description: "识别长期反复出现的方向",
    prompt:
      "哪些方向在我的素材里长期反复出现？请提炼共性、情绪、审美和跨界用途，并引用原始来源。",
  },
  {
    id: "priority_reason",
    slashName: "/why-priority",
    description: "解释为什么某个主题优先级高",
    prompt:
      "为什么这个主题优先级高？请从趋势、内驱力、系统关键解、边际成本/复利效应四个角度判断，并引用来源。",
  },
  {
    id: "brief",
    slashName: "/theme-brief",
    description: "把一个聚类结晶成主题 brief",
    prompt:
      "把这个聚类结晶成一个主题 brief：洞察、证据、反复元素、情绪/审美线索、跨界用途、优先级、冷却候选、下一步。",
  },
];
