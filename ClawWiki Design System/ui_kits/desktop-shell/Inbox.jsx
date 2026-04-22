/* global React, GitMerge, Plus, Copy, Flag */
const { useState: useStateI } = React;

const INBOX_ITEMS = [
  {
    id: 1,
    title: '合并冲突：«CCD 4 件套» 与 «Claude Code 开发流»',
    preview: 'Maintainer 发现两页标题相似、引用同源 — 建议合并为一页，保留两条原始引用。',
    when: '13:55',
    kind: 'merge',
    diff: `- wiki/CCD-4件套.md          (342 chars, 3 sources)
- wiki/Claude-Code-开发流.md (418 chars, 4 sources)
+ 提议合并为 wiki/CCD-开发流.md
  • 保留 4 件套小节
  • 新增「与 Cursor 对比」段落  (来自 raw/2026-04-18/2.md)
  • 去除重复的「基本循环」段`,
  },
  {
    id: 2,
    title: '新主题：«手术刀 vs 瑞士军刀»',
    preview: '4 条素材指向同一个想法。Maintainer 建议新建一页，归入 / 产品哲学 / 设计主张。',
    when: '13:40',
    kind: 'new',
    diff: `+ wiki/手术刀-vs-瑞士军刀.md (新)
  sources:
   • raw/2026-04-19/PPT-p7.png
   • raw/2026-04-19/公众号-漏斗论.md
   • raw/2026-04-18/语音转写-05.txt
   • raw/2026-04-17/1.md`,
  },
  {
    id: 3,
    title: '去重：«周记 2026-W16» 疑似 2 条',
    preview: '两条 raw/ 几乎等价 — 建议保留更长版本，另一条移至 raw/dup/。',
    when: '11:02',
    kind: 'dedupe',
    diff: `- raw/2026-04-15/周记.md    (保留 · 更长)
- raw/2026-04-16/周记.md    (→ raw/dup/)`,
  },
];

function InboxPage() {
  const [selected, setSelected] = useStateI(INBOX_ITEMS[0]);

  return (
    <div className="fade-in" style={{ display: 'grid', gridTemplateColumns: '400px 1fr', height: '100%' }}>
      <div style={{ borderRight: '1px solid var(--border-cream)', display: 'flex', flexDirection: 'column', overflow: 'hidden' }}>
        <div style={{ padding: '22px 20px 14px', borderBottom: '1px solid var(--border-cream)' }}>
          <div className="h2">Inbox</div>
          <div className="muted-txt" style={{ fontSize: 13, marginTop: 4 }}>Maintainer 的提议 · 3 待审阅</div>
          <div style={{ display: 'flex', gap: 6, marginTop: 14 }}>
            <span className="badge warn"><Flag size="3" /> 冲突 · 1</span>
            <span className="badge terra"><Plus size="3" /> 新页 · 1</span>
            <span className="badge muted"><Copy size="3" /> 去重 · 1</span>
          </div>
        </div>
        <div style={{ flex: 1, overflowY: 'auto' }}>
          {INBOX_ITEMS.map(item => (
            <div key={item.id}
                 className="inbox-row"
                 onClick={() => setSelected(item)}
                 style={{ background: selected.id === item.id ? 'var(--ivory)' : 'transparent' }}>
              <div className="diamond" style={{ color: item.kind === 'new' ? 'var(--claude-orange)' : item.kind === 'merge' ? 'var(--warning)' : 'var(--fg-4)', display: 'flex', alignItems: 'center', justifyContent: 'center' }}>
                {item.kind === 'new' ? <Plus size="4" /> : item.kind === 'merge' ? <Flag size="3.5" /> : <Copy size="3.5" />}
              </div>
              <div style={{ minWidth: 0 }}>
                <div className="title">{item.title}</div>
                <div className="preview">{item.preview}</div>
              </div>
              <div className="when">{item.when}</div>
            </div>
          ))}
        </div>
      </div>

      <div style={{ padding: '26px 32px', overflowY: 'auto' }}>
        <div className="overline">§ 2 · 合并 · 13:55</div>
        <div className="h2" style={{ marginTop: 6 }}>{selected.title}</div>
        <p style={{ fontSize: 14, color: 'var(--fg-2)', maxWidth: 680, marginTop: 10 }}>{selected.preview}</p>

        <div className="card" style={{ marginTop: 22, padding: 0, overflow: 'hidden' }}>
          <div style={{ padding: '10px 14px', borderBottom: '1px solid var(--border-cream)', display: 'flex', alignItems: 'center', justifyContent: 'space-between' }}>
            <span className="overline">提议 · diff</span>
            <span className="caption">Opus 4.5 · confidence 0.89</span>
          </div>
          <pre style={{ margin: 0, padding: '14px 18px', fontFamily: 'var(--font-mono)', fontSize: 12.5, lineHeight: 1.65, color: 'var(--fg-2)', background: 'var(--parchment)', whiteSpace: 'pre-wrap' }}>
{selected.diff}
          </pre>
        </div>

        <div style={{ display: 'flex', gap: 10, marginTop: 22 }}>
          <button className="btn primary">批准并合并</button>
          <button className="btn secondary">编辑提议</button>
          <button className="btn ghost">拒绝</button>
          <div style={{ flex: 1 }} />
          <button className="btn ghost">跳过 · 下一条 →</button>
        </div>
      </div>
    </div>
  );
}

Object.assign(window, { InboxPage });
