/* global React, GitMerge, Flag, Sparkles, Copy, ArrowRight */
const { useState: useStateD } = React;

// ─── StatCard ─────────────────────────────────────────────────
function StatCard({ overline, value, caption, delta, accent }) {
  return (
    <div className="card" style={{ padding: 20 }}>
      <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'flex-start' }}>
        <div className="overline">{overline}</div>
        {delta && <span className="badge muted tabular" style={{ fontFamily: 'var(--font-mono)', fontSize: 10 }}>{delta}</span>}
      </div>
      <div className="h-serif tabular" style={{ fontSize: 36, lineHeight: 1.1, marginTop: 8, color: accent || 'var(--fg-1)' }}>{value}</div>
      <div className="caption" style={{ marginTop: 4 }}>{caption}</div>
    </div>
  );
}

function ActivityRow({ Icon, iconColor, title, meta, when }) {
  return (
    <div style={{ display: 'grid', gridTemplateColumns: '28px 1fr auto', gap: 12, alignItems: 'center', padding: '12px 16px', borderTop: '1px solid var(--border-cream)' }}>
      <div style={{ color: iconColor || 'var(--fg-3)', display: 'flex', alignItems: 'center', justifyContent: 'center' }}>
        <Icon size="3.5" />
      </div>
      <div style={{ minWidth: 0 }}>
        <div style={{ fontSize: 13.5, color: 'var(--fg-1)', fontWeight: 500 }}>{title}</div>
        <div className="caption" style={{ color: 'var(--fg-4)' }}>{meta}</div>
      </div>
      <div style={{ fontFamily: 'var(--font-mono)', fontSize: 11, color: 'var(--fg-4)' }}>{when}</div>
    </div>
  );
}

function DashboardPage({ onJumpToInbox, onAsk }) {
  return (
    <div className="content fade-in">
      <div style={{ marginBottom: 28 }}>
        <div className="overline">2026-04-20 · 周一 · 14:23</div>
        <div className="h1" style={{ marginTop: 6 }}>早上好，你的外脑。</div>
        <p className="muted-txt" style={{ fontSize: 15.5, marginTop: 8, maxWidth: 620 }}>
          过去 24 小时里，Maintainer 审阅了 142 条素材，合并了 12 页 wiki，标记了 3 处冲突等你定夺。
        </p>
        <div style={{ display: 'flex', gap: 10, marginTop: 16 }}>
          <button className="btn primary" onClick={onAsk}>问 Wiki 一个问题 <ArrowRight size="3.5" /></button>
          <button className="btn secondary" onClick={onJumpToInbox}>查看待审阅 · 3</button>
        </div>
      </div>

      <div style={{ display: 'grid', gridTemplateColumns: 'repeat(4, 1fr)', gap: 14, marginBottom: 26 }}>
        <StatCard overline="今日入库"  value="142" caption="条 · 24h"        delta="+28" />
        <StatCard overline="待审阅"    value="3"   caption="合并冲突"        accent="var(--claude-orange)" />
        <StatCard overline="Wiki 规模" value="1,284" caption="pages · 342 nodes" delta="+12" />
        <StatCard overline="Maintainer" value="idle" caption="next pass 07:42" />
      </div>

      <div style={{ display: 'grid', gridTemplateColumns: '1.5fr 1fr', gap: 14 }}>
        <div className="card">
          <div style={{ padding: '14px 16px 10px', display: 'flex', justifyContent: 'space-between', alignItems: 'center' }}>
            <div className="h3">最近活动</div>
            <span className="caption">Maintainer · today</span>
          </div>
          <ActivityRow Icon={GitMerge} iconColor="var(--success)"       title="合并 «Karpathy 三层 · raw/wiki/schema»" meta="from 周日 PPT · 4 sources" when="14:12" />
          <ActivityRow Icon={Flag}     iconColor="var(--warning)"       title="冲突：«CCD 4 件套» 与 «Claude Code 开发流»" meta="Inbox §2 · awaits you" when="13:55" />
          <ActivityRow Icon={Sparkles} iconColor="var(--claude-orange)" title="新素材：mp.weixin.qq.com/… «手术刀，不是瑞士军刀»" meta="ingested · 3.4s" when="13:40" />
          <ActivityRow Icon={Copy}     iconColor="var(--fg-3)"          title="去重：«周记 2026-W16» × 2 合并" meta="automatic" when="11:02" />
          <ActivityRow Icon={Sparkles} iconColor="var(--fg-3)"          title="索引重建完成" meta="1,284 pages embedded" when="08:31" />
        </div>

        <div className="card">
          <div style={{ padding: '14px 16px 10px' }}>
            <div className="h3">常问</div>
          </div>
          <div style={{ padding: '0 16px 16px', display: 'flex', flexDirection: 'column', gap: 8 }}>
            {[
              '本周我最关心的主题是什么？',
              '把 Karpathy 三层结构讲给我妈听。',
              '上次那条关于 PPT 设计的金句原话是？',
              'Claude Code 和 Cursor 的区别，基于我看过的材料。',
            ].map((q, i) => (
              <button key={i} className="btn ghost" style={{ justifyContent: 'flex-start', textAlign: 'left', padding: '9px 12px', border: '1px solid var(--border-cream)', borderRadius: 'var(--radius-md)', fontSize: 13, color: 'var(--fg-2)', fontWeight: 400, lineHeight: 1.5 }}>
                <span style={{ color: 'var(--claude-orange)', marginRight: 6, display: 'inline-flex' }}><ArrowRight size="3.5" /></span>{q}
              </button>
            ))}
          </div>
        </div>
      </div>
    </div>
  );
}

Object.assign(window, { DashboardPage });
