/* global React, Sidebar, TopBar, DashboardPage, AskPage, InboxPage */
const { useState: useShellState } = React;

const CRUMBS = {
  dashboard: ['ClawWiki', 'Dashboard'],
  ask:       ['ClawWiki', 'Ask'],
  inbox:     ['ClawWiki', 'Inbox', '§ 2 合并'],
  raw:       ['ClawWiki', 'Raw'],
  wiki:      ['ClawWiki', 'Wiki'],
  graph:     ['ClawWiki', 'Graph'],
  schema:    ['ClawWiki', 'Schema'],
  bridge:    ['ClawWiki', 'WeChat Bridge'],
  settings:  ['ClawWiki', 'Settings'],
};

function PlaceholderPage({ title, note }) {
  return (
    <div className="content fade-in">
      <div className="overline">未在此演示中实现</div>
      <div className="h2" style={{ marginTop: 6 }}>{title}</div>
      <p className="muted-txt" style={{ fontSize: 14, maxWidth: 520, marginTop: 10 }}>{note}</p>
      <div className="card" style={{ marginTop: 24, padding: 24, display: 'grid', placeItems: 'center', minHeight: 240 }}>
        <div style={{ color: 'var(--fg-4)', fontSize: 28, fontFamily: 'var(--font-serif)' }}>◇</div>
        <div className="caption" style={{ marginTop: 8 }}>See the codebase for the shipped implementation</div>
      </div>
    </div>
  );
}

function App() {
  const [route, setRoute] = useShellState('dashboard');

  const go = (r) => setRoute(r);

  const content = (() => {
    switch (route) {
      case 'dashboard': return <DashboardPage onJumpToInbox={() => go('inbox')} onAsk={() => go('ask')} />;
      case 'ask':       return <AskPage />;
      case 'inbox':     return <InboxPage />;
      case 'raw':       return <PlaceholderPage title="Raw · 原料仓" note="按天分组的素材来源：WeChat、语音、PPT、URL。在完整版中可点进原文。" />;
      case 'wiki':      return <PlaceholderPage title="Wiki · 页面" note="Karpathy 三层中间层：归纳后的主题页，双链渲染，edit-in-place。" />;
      case 'graph':     return <PlaceholderPage title="Graph · 认知图" note="wiki 页面之间的引用、共现和 schema 边。force-directed layout。" />;
      case 'schema':    return <PlaceholderPage title="Schema · 本体" note="类型、关系、必填字段。Karpathy 三层的最顶层。" />;
      case 'bridge':    return <PlaceholderPage title="WeChat Bridge" note="外联机器人连接状态、白名单、今日吞吐。" />;
      case 'settings':  return <PlaceholderPage title="Settings" note="模型、存储、主题、快捷键。" />;
      default:          return null;
    }
  })();

  return (
    <div className="shell">
      <Sidebar route={route} onNavigate={go} pendingCount={3} />
      <div className="main">
        <TopBar
          crumbs={CRUMBS[route]}
          actions={
            <div style={{ display: 'flex', gap: 6, alignItems: 'center' }}>
              <span className="badge ok"><span className="status-dot" style={{ display: 'inline-block', width: 6, height: 6, borderRadius: '50%', background: 'currentColor', marginRight: 4 }} />healthy</span>
              <button className="btn ghost" title="Settings">⚙</button>
            </div>
          }
        />
        {content}
      </div>
    </div>
  );
}

const root = ReactDOM.createRoot(document.getElementById('root'));
root.render(<App />);
