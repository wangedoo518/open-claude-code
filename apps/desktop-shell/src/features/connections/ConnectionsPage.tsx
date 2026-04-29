import {
  Bot,
  CheckCircle2,
  GitBranch,
  LockKeyhole,
  MessageCircle,
  ShieldAlert,
} from "lucide-react";
import { Link } from "react-router-dom";

const CONNECTIONS = [
  {
    id: "wechat",
    label: "微信入口",
    description: "消息、文章、URL 的主要捕获入口。",
    status: "待检查",
    tone: "warning",
    icon: MessageCircle,
    href: "/wechat",
  },
  {
    id: "git",
    label: "Buddy Vault / Git",
    description: "新建 Vault 默认初始化 Git，所有写入都应能被 diff 和回滚。",
    status: "默认启用",
    tone: "success",
    icon: GitBranch,
    href: "/settings?tab=data",
  },
  {
    id: "external-ai",
    label: "外部 AI 受控写入",
    description: "默认只读；写入 wiki/schema/templates/root guidance 前需要授权。",
    status: "只读",
    tone: "neutral",
    icon: Bot,
    href: "/rules",
  },
] as const;

const WRITE_SCOPES = [
  "wiki/",
  "schema/templates",
  "AGENTS.md / CLAUDE.md",
  "当前选中页面",
] as const;

export function ConnectionsPage() {
  return (
    <main className="min-h-full overflow-y-auto bg-background px-6 py-5 text-foreground">
      <div className="mx-auto flex w-full max-w-6xl flex-col gap-5">
        <header className="flex flex-wrap items-end justify-between gap-3 border-b border-border/50 pb-4">
          <div>
            <div className="text-[11px] uppercase tracking-[0.14em] text-muted-foreground">
              Connections
            </div>
            <h1 className="mt-1 text-[22px] font-semibold tracking-normal">
              连接
            </h1>
            <p className="mt-1 max-w-2xl text-[13px] leading-6 text-muted-foreground">
              微信、模型、Git、MCP 与外部 AI 都在这里显式连接、授权和撤销。
            </p>
          </div>
          <Link
            to="/wechat"
            className="inline-flex h-9 items-center gap-2 rounded-md bg-primary px-3 text-[13px] text-primary-foreground"
          >
            <MessageCircle className="size-4" />
            连接微信
          </Link>
        </header>

        <section className="grid gap-3 lg:grid-cols-3">
          {CONNECTIONS.map((item) => {
            const Icon = item.icon;
            return (
              <Link
                key={item.id}
                to={item.href}
                className="rounded-lg border border-border bg-card px-4 py-4 text-card-foreground transition-colors hover:border-primary/40 hover:bg-muted/30"
              >
                <div className="flex items-start justify-between gap-3">
                  <span className="grid size-9 place-items-center rounded-md bg-muted text-muted-foreground">
                    <Icon className="size-4" />
                  </span>
                  <StatusBadge tone={item.tone}>{item.status}</StatusBadge>
                </div>
                <h2 className="mt-4 text-[15px] font-medium">{item.label}</h2>
                <p className="mt-2 text-[12px] leading-5 text-muted-foreground">
                  {item.description}
                </p>
              </Link>
            );
          })}
        </section>

        <section className="grid gap-4 lg:grid-cols-[minmax(0,1fr)_320px]">
          <div className="rounded-lg border border-border bg-card px-5 py-5">
            <div className="flex items-center gap-2">
              <LockKeyhole className="size-4 text-primary" />
              <h2 className="text-[15px] font-medium">受控自动写入授权</h2>
            </div>
            <div className="mt-4 grid gap-3 md:grid-cols-2">
              <AuthLevel
                title="本次会话有效"
                description="只在当前 app session 或 agent task 内生效，结束后自动回到只读。"
                badge="推荐"
              />
              <AuthLevel
                title="永久规则"
                description="写入 Rules/Connections，可撤销、可审计，并在 StatusBar 长期显示授权 badge。"
                badge="高风险"
              />
            </div>
          </div>

          <div className="rounded-lg border border-border bg-card px-5 py-5">
            <div className="flex items-center gap-2">
              <ShieldAlert className="size-4 text-[var(--color-warning)]" />
              <h2 className="text-[15px] font-medium">允许写入范围</h2>
            </div>
            <div className="mt-4 space-y-2">
              {WRITE_SCOPES.map((scope) => (
                <div
                  key={scope}
                  className="flex items-center gap-2 rounded-md bg-muted/50 px-3 py-2 text-[12px]"
                >
                  <CheckCircle2 className="size-3.5 text-[var(--color-success)]" />
                  <span>{scope}</span>
                </div>
              ))}
            </div>
          </div>
        </section>
      </div>
    </main>
  );
}

function StatusBadge({
  tone,
  children,
}: {
  tone: "success" | "warning" | "neutral";
  children: string;
}) {
  const cls =
    tone === "success"
      ? "bg-[var(--color-success)]/10 text-[var(--color-success)]"
      : tone === "warning"
        ? "bg-[var(--color-warning)]/10 text-[var(--color-warning)]"
        : "bg-muted text-muted-foreground";
  return (
    <span className={`rounded px-2 py-1 text-[11px] leading-none ${cls}`}>
      {children}
    </span>
  );
}

function AuthLevel({
  title,
  description,
  badge,
}: {
  title: string;
  description: string;
  badge: string;
}) {
  return (
    <div className="rounded-md border border-border/70 bg-background px-4 py-4">
      <div className="flex items-center justify-between gap-3">
        <h3 className="text-[13px] font-medium">{title}</h3>
        <span className="rounded bg-muted px-2 py-1 text-[11px] text-muted-foreground">
          {badge}
        </span>
      </div>
      <p className="mt-2 text-[12px] leading-5 text-muted-foreground">
        {description}
      </p>
    </div>
  );
}
