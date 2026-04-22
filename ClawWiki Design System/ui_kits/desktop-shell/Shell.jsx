/* global React, Home, MessageCircle, Inbox, FileStack, BookOpen, Network, Sigma, Link, Settings, Search */
const { useState } = React;

// ─── Sidebar ──────────────────────────────────────────────────
function Sidebar({ route, onNavigate, pendingCount }) {
  const items = [
    { id: 'dashboard', Icon: Home,          label: 'Dashboard' },
    { id: 'ask',       Icon: MessageCircle, label: 'Ask' },
    { id: 'inbox',     Icon: Inbox,         label: 'Inbox', pill: pendingCount },
    { id: 'raw',       Icon: FileStack,     label: 'Raw' },
    { id: 'wiki',      Icon: BookOpen,      label: 'Wiki' },
    { id: 'graph',     Icon: Network,       label: 'Graph' },
    { id: 'schema',    Icon: Sigma,         label: 'Schema' },
  ];
  const tools = [
    { id: 'bridge',   Icon: Link,     label: 'WeChat Bridge' },
    { id: 'settings', Icon: Settings, label: 'Settings' },
  ];
  return (
    <aside className="sidebar">
      <div className="brand">
        <div className="mark">C</div>
        <div className="name">ClawWiki</div>
      </div>
      <div className="group-label">Workspace</div>
      <nav>
        {items.map(i => {
          const IconComp = i.Icon;
          return (
            <a key={i.id}
               className={route === i.id ? 'active' : ''}
               onClick={e => { e.preventDefault(); onNavigate(i.id); }}
               href="#">
              <span className="glyph"><IconComp size="3.5" /></span>
              <span>{i.label}</span>
              {i.pill ? <span className="pill">{i.pill}</span> : null}
            </a>
          );
        })}
      </nav>
      <div className="group-label">Tools</div>
      <nav>
        {tools.map(i => {
          const IconComp = i.Icon;
          return (
            <a key={i.id}
               className={route === i.id ? 'active' : ''}
               onClick={e => { e.preventDefault(); onNavigate(i.id); }}
               href="#">
              <span className="glyph"><IconComp size="3.5" /></span>
              <span>{i.label}</span>
            </a>
          );
        })}
      </nav>
      <div style={{ flex: 1 }} />
      <div className="caption" style={{ padding: '10px', color: 'var(--fg-4)' }}>
        <span style={{ display: 'inline-block', width: 6, height: 6, borderRadius: '50%', background: 'var(--success)', marginRight: 6, verticalAlign: 'middle' }} />
        bridge · online
      </div>
    </aside>
  );
}

// ─── TopBar ───────────────────────────────────────────────────
function TopBar({ crumbs = [], actions }) {
  return (
    <div className="topbar">
      <div className="crumb">
        {crumbs.map((c, i) => (
          <React.Fragment key={i}>
            {i > 0 && <span className="sep">/</span>}
            <span style={{ color: i === crumbs.length - 1 ? 'var(--fg-1)' : 'var(--fg-3)' }}>{c}</span>
          </React.Fragment>
        ))}
      </div>
      <div className="spacer" />
      <div className="search">
        <Search size="3.5" />
        <span>搜索 wiki …</span>
        <span className="kbd">⌘K</span>
      </div>
      {actions}
    </div>
  );
}

Object.assign(window, { Sidebar, TopBar });
