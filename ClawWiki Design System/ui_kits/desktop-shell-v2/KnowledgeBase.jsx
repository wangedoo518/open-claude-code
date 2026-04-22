/* global React, BookOpen, Network, FileStack, Folder, FileText, Tag, Clock */

const PAGES = [
  { title: '产品哲学 · 手术刀，不是瑞士军刀', folder: '产品哲学', sources: 4, updated: '今天' },
  { title: 'CCD 四件套', folder: '开发方法', sources: 3, updated: '昨天' },
  { title: 'Claude Code 开发流', folder: '开发方法', sources: 4, updated: '昨天' },
  { title: 'Karpathy 三层结构 · raw / wiki / schema', folder: '认知模型', sources: 5, updated: '3 天前' },
  { title: '周记 · 2026-W16', folder: '日记', sources: 2, updated: '3 天前' },
  { title: 'AI 编程工具对比 · Claude Code vs Cursor', folder: '开发方法', sources: 6, updated: '一周前' },
];

const RAW = [
  { icon: '💬', title: '《手术刀，不是瑞士军刀》', source: '公众号转发', when: '今天 13:40' },
  { icon: '🎤', title: '关于「产品哲学」的语音备忘 · 2 分 14 秒', source: '语音', when: '昨天 21:02' },
  { icon: '📎', title: '周日团队 PPT · 14 页', source: '文件转发', when: '4 月 19 日' },
  { icon: '💬', title: '《CCD 基本循环》', source: '公众号转发', when: '4 月 16 日' },
];

function KnowledgeBase({ tab }) {
  if (tab === 'graph') return <GraphView />;

  return (
    <div className="kb-page fade-in">
      {tab === 'pages' && (
        <>
          <div style={{ marginBottom: 18 }}>
            <div className="h2">已整理的页面</div>
            <div className="muted-txt" style={{ fontSize: 13, marginTop: 4 }}>
              共 <b style={{ color: 'var(--fg-1)' }}>1,284</b> 页 · 按最近更新排序
            </div>
          </div>
          {PAGES.map((p, i) => (
            <div key={i} className="list-item">
              <div className="l-ico"><FileText size="3.5" /></div>
              <div style={{ minWidth: 0 }}>
                <div className="l-title">{p.title}</div>
                <div className="l-sub">
                  <Folder size="3" style={{ display: 'inline', verticalAlign: '-2px', marginRight: 4, color: 'var(--fg-4)' }} />
                  {p.folder} · 来自 {p.sources} 条原始内容
                </div>
              </div>
              <div className="l-meta"><Clock size="3" /> {p.updated}</div>
            </div>
          ))}
        </>
      )}

      {tab === 'raw' && (
        <>
          <div style={{ marginBottom: 18 }}>
            <div className="h2">素材库</div>
            <div className="muted-txt" style={{ fontSize: 13, marginTop: 4 }}>
              你转发进来的原始内容，按时间排序。这些会自动整理到页面里。
            </div>
          </div>
          {RAW.map((r, i) => (
            <div key={i} className="list-item">
              <div className="l-ico" style={{ fontSize: 16 }}>{r.icon}</div>
              <div style={{ minWidth: 0 }}>
                <div className="l-title">{r.title}</div>
                <div className="l-sub"><Tag size="3" style={{ display: 'inline', verticalAlign: '-2px', marginRight: 4, color: 'var(--fg-4)' }} />{r.source}</div>
              </div>
              <div className="l-meta">{r.when}</div>
            </div>
          ))}
        </>
      )}
    </div>
  );
}

function GraphView() {
  const nodes = [
    { id: 'a', x: 320, y: 180, label: '产品哲学', main: true },
    { id: 'b', x: 510, y: 130, label: '手术刀 vs 瑞士军刀' },
    { id: 'c', x: 240, y: 300, label: 'CCD 四件套' },
    { id: 'd', x: 450, y: 310, label: 'Claude Code 开发流' },
    { id: 'e', x: 630, y: 240, label: 'Karpathy 三层' },
    { id: 'f', x: 600, y: 380, label: 'AI 工具对比' },
    { id: 'g', x: 180, y: 180, label: '周记 W16' },
  ];
  const edges = [['a','b'],['a','c'],['a','g'],['c','d'],['d','e'],['d','f'],['b','e']];
  const byId = Object.fromEntries(nodes.map(n => [n.id, n]));

  return (
    <div className="kb-page fade-in">
      <div style={{ marginBottom: 18 }}>
        <div className="h2">关系图</div>
        <div className="muted-txt" style={{ fontSize: 13, marginTop: 4 }}>
          页面之间的关联。点一下任意节点看详情。
        </div>
      </div>
      <div className="card" style={{ padding: 0, height: 480, overflow: 'hidden' }}>
        <svg viewBox="0 0 800 480" width="100%" height="100%">
          <defs>
            <pattern id="dot2" x="0" y="0" width="24" height="24" patternUnits="userSpaceOnUse">
              <circle cx="1" cy="1" r="1" fill="var(--border-warm)" />
            </pattern>
          </defs>
          <rect width="800" height="480" fill="url(#dot2)" opacity="0.5" />
          {edges.map(([a, b], i) => (
            <line key={i} x1={byId[a].x} y1={byId[a].y} x2={byId[b].x} y2={byId[b].y} stroke="var(--border-warm)" strokeWidth="1.5" />
          ))}
          {nodes.map(n => (
            <g key={n.id} transform={`translate(${n.x}, ${n.y})`} style={{ cursor: 'pointer' }}>
              <circle r={n.main ? 11 : 6} fill={n.main ? 'var(--claude-orange)' : 'var(--ivory)'} stroke={n.main ? 'var(--claude-orange)' : 'var(--fg-3)'} strokeWidth="1.5" />
              <text x="14" y="4" fontFamily="var(--font-sans)" fontSize="12" fill="var(--fg-1)">{n.label}</text>
            </g>
          ))}
        </svg>
      </div>
    </div>
  );
}

Object.assign(window, { KnowledgeBase });
