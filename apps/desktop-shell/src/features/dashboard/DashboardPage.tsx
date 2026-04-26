import {
  useEffect,
  useMemo,
  useState,
  type CSSProperties,
  type ReactNode,
} from "react";
import {
  Bell,
  BookOpen,
  Bookmark,
  Check,
  ChevronRight,
  MessageCircle,
  Pencil,
  RefreshCw,
  Search,
  SearchCheck,
  X,
} from "lucide-react";
import {
  getStoredProfileName,
  normalizeProfileName,
  saveStoredProfileName,
} from "@/lib/profile-name";

const maintainerItems = [
  {
    title: "Claude Code Windows 代理配置",
    source: "来自 微信文章 91",
    tone: "#5D7FA3",
  },
  {
    title: "注册流程头像唯一性决策",
    source: "来自 产品讨论 87",
    tone: "#534AB7",
  },
  {
    title: "a2hmarket skill 安装流程",
    source: "来自 微信文章 95",
    tone: "#1D9E75",
  },
];

const metrics = [
  {
    label: "今日入库",
    value: 6,
    suffix: "/ 29",
    pill: "+2",
    pillClass: "bg-[#E1F5EE] text-[#0F6E56]",
    progress: 21,
    color: "#D85A30",
  },
  {
    label: "本周新增",
    value: 7,
    suffix: "页知识",
    pill: "+12%",
    pillClass: "bg-[#E1F5EE] text-[#0F6E56]",
    progress: 58,
    color: "#1D9E75",
  },
  {
    label: "知识库覆盖度",
    value: 73,
    suffix: "%",
    pill: "良好",
    pillClass: "bg-[#EEEDFE] text-[#534AB7]",
    progress: 73,
    color: "#534AB7",
  },
];

const quickActions = [
  { label: "向外脑提问", href: "#/ask", icon: MessageCircle },
  { label: "打开知识库", href: "#/wiki", icon: BookOpen },
  { label: "知识体检", href: "#/wiki?view=dashboard", icon: SearchCheck },
];

export function DashboardPage() {
  const now = useMemo(() => new Date(), []);
  const greeting = formatGreeting(now);
  const statusTime = formatStatusTime(now);
  const weekday = formatChineseWeekday(now);
  const [profileName, setProfileName] = useState(getStoredProfileName);
  const [isEditingProfileName, setIsEditingProfileName] = useState(false);
  const [draftProfileName, setDraftProfileName] = useState(profileName);
  const canSaveProfileName = draftProfileName.trim().length > 0;

  useEffect(() => {
    setDraftProfileName(profileName);
  }, [profileName]);

  const startEditingProfileName = () => {
    setDraftProfileName(profileName);
    setIsEditingProfileName(true);
  };

  const cancelEditingProfileName = () => {
    setDraftProfileName(profileName);
    setIsEditingProfileName(false);
  };

  const saveProfileName = () => {
    if (!canSaveProfileName) return;
    const nextName = normalizeProfileName(draftProfileName) ?? profileName;
    setProfileName(saveStoredProfileName(nextName));
    setIsEditingProfileName(false);
  };

  return (
    <main className="min-h-full overflow-y-auto bg-[#FAF8F3] px-8 pb-12 pt-4 text-[#2C2C2A] [font-family:Inter,'Source_Han_Sans_SC','Noto_Sans_SC',system-ui,sans-serif]">
      <OuterBrainStyles />
      <div className="mx-auto flex w-full max-w-[1080px] flex-col gap-8">
        <section
          className="outer-section flex h-8 items-center justify-between text-[13px] leading-none"
          style={sectionDelay(0)}
        >
          <div className="flex items-center gap-2 text-[#5F5E5A]">
            <span className="outer-pulse-dot size-1.5 rounded-full bg-[#1D9E75]" />
            <span>微信已连接</span>
            <span className="mx-1 h-3 w-px bg-[#888780]/30" aria-hidden="true" />
            <span>{profileName} 的外脑</span>
            <span className="mx-1 h-3 w-px bg-[#888780]/30" aria-hidden="true" />
            <span className="text-[#888780]">{statusTime}</span>
          </div>
          <div className="flex items-center gap-4 text-[#5F5E5A]">
            <Search className="size-[13px]" strokeWidth={1.8} aria-hidden="true" />
            <span className="relative inline-flex">
              <Bell className="size-[13px]" strokeWidth={1.8} aria-hidden="true" />
              <span className="absolute -right-0.5 -top-0.5 size-1.5 rounded-full bg-[#D85A30]" />
            </span>
          </div>
        </section>

        <section className="outer-section" style={sectionDelay(1)}>
          <div className="text-[11px] font-normal uppercase leading-none tracking-[0.1em] text-[#888780]">
            TODAY · {weekday}
          </div>
          <div className="mt-3 flex flex-wrap items-end gap-3">
            <h1 className="font-serif text-[32px] font-medium leading-[1.15] tracking-[-0.02em] text-[#2C2C2A]">
              {greeting}，<span className="text-[#D85A30]">{profileName}</span>
            </h1>
            {!isEditingProfileName && (
              <button
                type="button"
                onClick={startEditingProfileName}
                className="outer-profile-edit-button mb-1 inline-flex items-center gap-1.5 rounded-lg border border-[rgba(44,44,42,0.15)] bg-white/70 px-2.5 py-1.5 text-[12px] font-normal leading-none text-[#5F5E5A] backdrop-blur transition-colors duration-280 hover:border-[rgba(44,44,42,0.26)] hover:bg-[#F1EFE8] hover:text-[#2C2C2A]"
                aria-label="修改首页用户名"
              >
                <Pencil className="size-3" strokeWidth={1.7} />
                修改姓名
              </button>
            )}
          </div>
          {isEditingProfileName && (
            <form
              className="outer-profile-editor mt-3 flex max-w-md flex-wrap items-center gap-2"
              onSubmit={(event) => {
                event.preventDefault();
                saveProfileName();
              }}
            >
              <label className="sr-only" htmlFor="dashboard-profile-name">
                首页用户名
              </label>
              <input
                id="dashboard-profile-name"
                value={draftProfileName}
                onChange={(event) => setDraftProfileName(event.target.value)}
                onKeyDown={(event) => {
                  if (event.key === "Escape") {
                    event.preventDefault();
                    cancelEditingProfileName();
                  }
                }}
                autoFocus
                maxLength={24}
                placeholder="输入你的名字"
                className="h-9 min-w-0 flex-1 rounded-lg border border-[rgba(44,44,42,0.15)] bg-white px-3 text-[13px] text-[#2C2C2A] outline-none transition-colors placeholder:text-[#888780] focus:border-[#D85A30]"
              />
              <button
                type="submit"
                disabled={!canSaveProfileName}
                className="inline-flex h-9 items-center gap-1.5 rounded-lg bg-[#2C2C2A] px-3 text-[13px] font-normal text-white transition-colors hover:bg-[#D85A30] disabled:cursor-not-allowed disabled:bg-[#888780]/40"
              >
                <Check className="size-3.5" strokeWidth={1.8} />
                保存
              </button>
              <button
                type="button"
                onClick={cancelEditingProfileName}
                className="inline-flex h-9 items-center gap-1.5 rounded-lg border border-[rgba(44,44,42,0.15)] bg-white px-3 text-[13px] font-normal text-[#5F5E5A] transition-colors hover:bg-[#F1EFE8] hover:text-[#2C2C2A]"
              >
                <X className="size-3.5" strokeWidth={1.8} />
                取消
              </button>
            </form>
          )}
          <p className="mt-3 text-[14px] font-normal leading-6 text-[#5F5E5A]">
            今天有 6 条等你判断，预计 3 分钟。
          </p>
        </section>

        <section
          className="outer-section overflow-hidden rounded-xl border border-[rgba(44,44,42,0.15)] bg-white px-7 py-6"
          style={sectionDelay(2)}
        >
          <div className="relative">
            <div
              className="pointer-events-none absolute -right-14 -top-20 h-56 w-56 rounded-full"
              style={{
                background:
                  "radial-gradient(circle, rgba(216,90,48,0.04), transparent 68%)",
              }}
            />
            <div className="relative flex h-8 items-center justify-between gap-4">
              <div className="flex min-w-0 items-center gap-2">
                <span className="size-1.5 shrink-0 rounded-full bg-[#D85A30]" />
                <span className="text-[13px] font-medium text-[#2C2C2A]">
                  待整理
                </span>
                <span className="text-[11px] font-normal text-[#888780]">
                  · 6 条 · 预计 3 分钟
                </span>
              </div>
              <span className="rounded px-2 py-1 text-[11px] font-normal leading-none text-[#D85A30] bg-[#FAECE7]">
                Maintainer 已审阅
              </span>
            </div>

            <div className="my-5 h-px bg-[rgba(44,44,42,0.12)]" />

            <div className="relative grid gap-6 lg:grid-cols-[minmax(0,1fr)_auto] lg:items-end">
              <div>
                <h2 className="font-serif text-[20px] font-medium leading-[1.35] tracking-[-0.01em] text-[#2C2C2A]">
                  Maintainer 给了 6 条建议，等你判断归类
                </h2>

                <div className="mt-5 grid gap-1.5">
                  {maintainerItems.map((item) => (
                    <a
                      key={item.title}
                      href="#/inbox"
                      className="outer-list-row grid h-9 grid-cols-[24px_minmax(0,1fr)_auto_auto] items-center gap-3 rounded-md px-2 text-decoration-none"
                    >
                      <span
                        className="grid size-5 place-items-center rounded-full"
                        style={{ backgroundColor: `${item.tone}1A` }}
                        aria-hidden="true"
                      >
                        <Bookmark
                          className="size-3"
                          strokeWidth={1.8}
                          style={{ color: item.tone }}
                        />
                      </span>
                      <span className="truncate text-[14px] font-normal text-[#2C2C2A]">
                        {item.title}
                      </span>
                      <span className="hidden truncate text-[12px] font-normal text-[#888780] md:inline">
                        {item.source}
                      </span>
                      <ChevronRight
                        className="size-3.5 text-[#888780]"
                        strokeWidth={1.7}
                        aria-hidden="true"
                      />
                    </a>
                  ))}
                </div>

                <a
                  href="#/inbox"
                  className="mt-2 inline-flex rounded-md px-2 py-1 text-[13px] font-normal text-[#5F5E5A] underline decoration-[#888780]/40 underline-offset-4 transition-colors duration-180 hover:text-[#D85A30]"
                >
                  还有 3 条…
                </a>
              </div>

              <a
                href="#/inbox"
                className="outer-cta inline-flex h-10 items-center justify-center gap-2 rounded-lg bg-[#2C2C2A] px-5 text-[13px] font-medium leading-none text-white transition-colors duration-280"
              >
                开始整理
                <ChevronRight className="outer-cta-arrow size-3.5" strokeWidth={1.8} />
              </a>
            </div>
          </div>
        </section>

        <section
          className="outer-section grid grid-cols-1 gap-3 md:grid-cols-3"
          style={sectionDelay(3)}
        >
          {metrics.map((metric) => (
            <MetricCard key={metric.label} {...metric} />
          ))}
        </section>

        <section
          className="outer-section rounded-xl bg-[rgba(44,44,42,0.025)] px-5 py-[18px]"
          style={sectionDelay(4)}
        >
          <div className="flex items-center justify-between">
            <h2 className="text-[13px] font-medium leading-none text-[#2C2C2A]">
              最近动态
            </h2>
            <div className="flex items-center gap-2 text-[11px] font-normal leading-none text-[#5F5E5A]">
              <span>实时</span>
              <span className="outer-live-dot size-[5px] rounded-full bg-[#1D9E75]" />
            </div>
          </div>

          <div className="mt-4 grid gap-2">
            <TimelineRow time="16:21" tone="green" icon={<Check className="size-2.5" strokeWidth={2} />}>
              整理完成 · <Mark>微信文章 98</Mark> 已加入
              <Accent>概念库</Accent>
            </TimelineRow>
            <TimelineRow
              time="16:18"
              tone="purple"
              icon={<RefreshCw className="size-2.5" strokeWidth={1.8} />}
            >
              关联建立 · 网页 61 与 <Accent>Claude API 概览</Accent>
            </TimelineRow>
            <TimelineRow
              time="现在"
              tone="plain"
              icon={<RefreshCw className="outer-spin-slow size-3" strokeWidth={1.6} />}
              italic
            >
              正在监听新消息…
            </TimelineRow>
          </div>
        </section>

        <section
          className="outer-section flex flex-col gap-4 md:flex-row md:items-center md:justify-between"
          style={sectionDelay(5)}
        >
          <div className="flex flex-wrap gap-2.5">
            {quickActions.map((action) => {
              const Icon = action.icon;
              return (
                <a
                  key={action.label}
                  href={action.href}
                  className="outer-quick-card inline-flex h-10 items-center gap-2 rounded-lg border border-[rgba(44,44,42,0.15)] bg-white px-3.5 text-[13px] font-normal text-[#2C2C2A] transition-colors duration-280"
                >
                  <Icon className="size-3.5 text-[#5F5E5A]" strokeWidth={1.7} />
                  {action.label}
                </a>
              );
            })}
          </div>
          <div className="text-[11px] font-normal leading-none text-[#888780]">
            ⌘K 唤起
          </div>
        </section>
      </div>
    </main>
  );
}

function MetricCard({
  label,
  value,
  suffix,
  pill,
  pillClass,
  progress,
  color,
}: (typeof metrics)[number]) {
  const animatedValue = useCountUp(value);

  return (
    <article className="outer-card-hover rounded-xl border border-[rgba(44,44,42,0.15)] bg-white px-[18px] py-4">
      <div className="flex items-center justify-between gap-3">
        <span className="text-[12px] font-normal leading-none text-[#5F5E5A]">
          {label}
        </span>
        <span
          className={`rounded px-1.5 py-1 text-[11px] font-normal leading-none ${pillClass}`}
        >
          {pill}
        </span>
      </div>

      <div className="mt-5 flex items-baseline gap-1.5">
        <span className="font-serif text-[28px] font-medium leading-none tracking-[-0.03em] text-[#2C2C2A]">
          {animatedValue}
        </span>
        <span className="text-[12px] font-normal leading-none text-[#888780]">
          {suffix}
        </span>
      </div>

      <div className="mt-4 h-[3px] overflow-hidden rounded-full bg-[rgba(44,44,42,0.08)]">
        <span
          className="outer-progress-fill block h-full origin-left rounded-full"
          style={
            {
              width: `${progress}%`,
              backgroundColor: color,
            } as CSSProperties
          }
        />
      </div>
    </article>
  );
}

function TimelineRow({
  time,
  tone,
  icon,
  children,
  italic = false,
}: {
  time: string;
  tone: "green" | "purple" | "plain";
  icon: ReactNode;
  children: ReactNode;
  italic?: boolean;
}) {
  const toneClass =
    tone === "green"
      ? "bg-[#E1F5EE] text-[#1D9E75]"
      : tone === "purple"
        ? "bg-[#EEEDFE] text-[#534AB7]"
        : "bg-transparent text-[#888780]";

  return (
    <div className="grid min-h-7 grid-cols-[42px_18px_minmax(0,1fr)] items-center gap-3">
      <span className="text-[12px] font-normal text-[#888780]">{time}</span>
      <span className={`grid size-4 place-items-center rounded-full ${toneClass}`}>
        {icon}
      </span>
      <p
        className={`text-[13px] font-normal leading-5 ${
          italic ? "italic text-[#888780]" : "text-[#2C2C2A]"
        }`}
      >
        {children}
      </p>
    </div>
  );
}

function Mark({ children }: { children: ReactNode }) {
  return (
    <span className="rounded bg-[rgba(44,44,42,0.055)] px-1 py-0.5 text-[#5F5E5A]">
      {children}
    </span>
  );
}

function Accent({ children }: { children: ReactNode }) {
  return <span className="text-[#D85A30]">{children}</span>;
}

function useCountUp(target: number, duration = 1100) {
  const [value, setValue] = useState(0);

  useEffect(() => {
    let frame = 0;
    const startedAt = performance.now();
    const tick = (now: number) => {
      const progress = Math.min(1, (now - startedAt) / duration);
      const eased = 1 - Math.pow(1 - progress, 3);
      setValue(Math.round(target * eased));
      if (progress < 1) {
        frame = requestAnimationFrame(tick);
      }
    };
    frame = requestAnimationFrame(tick);
    return () => cancelAnimationFrame(frame);
  }, [duration, target]);

  return value;
}

function sectionDelay(index: number): CSSProperties {
  return { animationDelay: `${index * 60}ms` };
}

function formatGreeting(date: Date): string {
  const hour = date.getHours();
  if (hour < 6) return "夜深了";
  if (hour < 12) return "上午好";
  if (hour < 18) return "下午好";
  return "晚上好";
}

function formatStatusTime(date: Date): string {
  const hour = date.getHours();
  const minute = String(date.getMinutes()).padStart(2, "0");
  const period = hour < 12 ? "上午" : "下午";
  const displayHour = hour % 12 === 0 ? 12 : hour % 12;
  return `${period} ${displayHour}:${minute}`;
}

function formatChineseWeekday(date: Date): string {
  return `周${["日", "一", "二", "三", "四", "五", "六"][date.getDay()]}`;
}

function OuterBrainStyles() {
  return (
    <style>{`
      @keyframes outer-section-enter {
        from { opacity: 0; transform: translateY(8px); }
        to { opacity: 1; transform: translateY(0); }
      }

      @keyframes outer-pulse {
        0%, 100% { box-shadow: 0 0 0 0 rgba(29, 158, 117, 0.24); }
        50% { box-shadow: 0 0 0 7px rgba(29, 158, 117, 0); }
      }

      @keyframes outer-live-pulse {
        0%, 100% { box-shadow: 0 0 0 0 rgba(29, 158, 117, 0.22); }
        50% { box-shadow: 0 0 0 6px rgba(29, 158, 117, 0); }
      }

      @keyframes outer-progress {
        from { transform: scaleX(0); }
        to { transform: scaleX(1); }
      }

      @keyframes outer-spin {
        to { transform: rotate(360deg); }
      }

      .outer-section {
        opacity: 0;
        animation: outer-section-enter 520ms cubic-bezier(0.2, 0, 0, 1) forwards;
      }

      .outer-pulse-dot {
        animation: outer-pulse 2s cubic-bezier(0.2, 0, 0, 1) infinite;
      }

      .outer-live-dot {
        animation: outer-live-pulse 1.5s cubic-bezier(0.2, 0, 0, 1) infinite;
      }

      .outer-card-hover {
        transition:
          transform 280ms cubic-bezier(0.2, 0, 0, 1),
          border-color 280ms cubic-bezier(0.2, 0, 0, 1);
      }

      .outer-card-hover:hover {
        transform: translateY(-2px);
        border-color: rgba(44, 44, 42, 0.26);
      }

      .outer-list-row {
        transition:
          background-color 180ms cubic-bezier(0.2, 0, 0, 1),
          color 180ms cubic-bezier(0.2, 0, 0, 1);
      }

      .outer-list-row:hover {
        background-color: rgba(216, 90, 48, 0.04);
      }

      .outer-cta {
        transition:
          background-color 280ms cubic-bezier(0.2, 0, 0, 1),
          color 280ms cubic-bezier(0.2, 0, 0, 1);
      }

      .outer-cta:hover {
        background-color: #D85A30;
      }

      .outer-cta-arrow {
        transition: transform 280ms cubic-bezier(0.2, 0, 0, 1);
      }

      .outer-cta:hover .outer-cta-arrow {
        transform: translateX(2px);
      }

      .outer-progress-fill {
        animation: outer-progress 1200ms cubic-bezier(0.2, 0, 0, 1) 200ms both;
      }

      .outer-spin-slow {
        animation: outer-spin 4s linear infinite;
      }

      .outer-quick-card {
        transition:
          background-color 280ms cubic-bezier(0.2, 0, 0, 1),
          border-color 280ms cubic-bezier(0.2, 0, 0, 1);
      }

      .outer-quick-card:hover {
        background-color: #F1EFE8;
        border-color: rgba(44, 44, 42, 0.26);
      }

      @media (prefers-reduced-motion: reduce) {
        .outer-section,
        .outer-pulse-dot,
        .outer-live-dot,
        .outer-progress-fill,
        .outer-spin-slow {
          animation: none;
          opacity: 1;
          transform: none;
        }
      }
    `}</style>
  );
}
