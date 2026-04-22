/* global React, Home, MessageCircle, Wand, BookOpen, Link, Settings, Sparkles, FileStack, Lightbulb, ChevronRight */
const { useState: useShellV2 } = React;

// Icon-only left rail (QClaw-style) — avatar top, nav, bottom utilities
function RailV2({ route, onNavigate, reviewCount }) {
  const items = [
    { id: 'chat',   Icon: MessageCircle, label: '对话' },
    { id: 'kb',     Icon: BookOpen,      label: '知识库' },
    { id: 'home',   Icon: Lightbulb,     label: '灵感',   dot: reviewCount > 0 },
  ];
  return (
    <aside className="rail">
      <div className="avatar-top" title="我的账号">Y</div>
      <div className="nav-items">
        {items.map(i => {
          const I = i.Icon;
          return (
            <button key={i.id} className={`rail-btn ${route === i.id ? 'active' : ''}`} onClick={() => onNavigate(i.id)}>
              <I size="5" />
              <span className="label">{i.label}</span>
              {i.dot && <span className="badge-dot" />}
            </button>
          );
        })}
      </div>
      <div className="spacer" />
      <div className="bottom-icons">
        <button title="微信接入" onClick={() => onNavigate('connect')}><Link size="4.5" /></button>
        <button title="设置" onClick={() => onNavigate('settings')}><Settings size="4.5" /></button>
      </div>
      <div className="update-card" title="发现新版本 v2.5">
        <span className="emoji">🎁</span>
        <span className="txt">发现新版本！</span>
        <span className="btn-tiny">更新</span>
      </div>
    </aside>
  );
}

// Top bar with pill-tabs (for KB) or plain title, + quota chip + history + avatar
function TopBarV2({ tabs, activeTab, onTab, rightExtra, title }) {
  return (
    <div className="topbar">
      {tabs ? (
        <div className="pill-tabs">
          {tabs.map(t => (
            <button key={t.id} className={`pt ${activeTab === t.id ? 'active' : ''}`} onClick={() => onTab(t.id)}>
              {t.icon ? <t.icon size="3.5" /> : null}
              {t.label}
            </button>
          ))}
        </div>
      ) : (
        <div className="h-serif" style={{ fontSize: 15 }}>{title}</div>
      )}
      <div className="top-right">
        {rightExtra}
        <div className="quota-chip">
          <span className="dot">◇</span>
          <span>今日未使用，剩余100%</span>
        </div>
        <button className="icon-btn" title="历史记录">
          <ClockGlyph />
        </button>
        <div className="avatar-chip" title="我的账号">Y</div>
      </div>
    </div>
  );
}

function ClockGlyph() {
  return (
    <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round">
      <circle cx="12" cy="12" r="10"/><polyline points="12 6 12 12 16 14"/>
    </svg>
  );
}

Object.assign(window, { RailV2, TopBarV2 });
