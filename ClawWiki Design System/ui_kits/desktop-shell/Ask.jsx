/* global React, BookOpen, FileStack, Network, Sparkles, ArrowRight */
const { useState: useStateA, useRef, useEffect } = React;

function ChatMessage({ role, content, streaming, sources }) {
  return (
    <div className={`chat-msg ${role} fade-in`}>
      <div style={{ maxWidth: '78%' }}>
        {role === 'ai' && <div className="meta">Maintainer · Opus 4.5</div>}
        <div className={`bubble ${streaming ? 'streaming' : ''}`}>
          <div>{content}</div>
          {sources && (
            <div style={{ marginTop: 10, paddingTop: 8, borderTop: '1px solid var(--border-cream)', display: 'flex', flexWrap: 'wrap', gap: 6 }}>
              {sources.map((s, i) => (
                <span key={i} className="badge muted" style={{ fontFamily: 'var(--font-mono)', fontSize: 10.5 }}>
                  ↗ {s}
                </span>
              ))}
            </div>
          )}
          {streaming && (
            <div className="streaming-dot"><span className="dot">⏺</span> streaming · 1.4s</div>
          )}
        </div>
      </div>
    </div>
  );
}

function Composer({ onSend, busy }) {
  const [val, setVal] = useStateA('');
  const ref = useRef(null);
  const send = () => {
    const v = val.trim();
    if (!v || busy) return;
    onSend(v);
    setVal('');
  };
  return (
    <div className="composer" style={{ margin: '12px 0 0' }}>
      <textarea
        ref={ref}
        value={val}
        onChange={e => setVal(e.target.value)}
        onKeyDown={e => { if (e.key === 'Enter' && (e.metaKey || e.ctrlKey)) { e.preventDefault(); send(); } }}
        placeholder="问一问你的外脑…  ⌘↵ 发送"
      />
      <div className="row">
        <span className="chip"><BookOpen size="3" /> wiki/</span>
        <span className="chip"><FileStack size="3" /> raw/ (24h)</span>
        <span className="chip"><Network size="3" /> graph</span>
        <div className="spacer" />
        <span className="caption" style={{ color: 'var(--fg-4)' }}>Opus 4.5 · 32k ctx</span>
        <button className="btn primary" disabled={busy || !val.trim()} onClick={send}>发送 <span style={{ opacity: .7, fontFamily: 'var(--font-mono)' }}>⌘↵</span></button>
      </div>
    </div>
  );
}

function AskPage() {
  const [messages, setMessages] = useStateA([
    { role: 'ai', content: '你好。这里是你的外脑 — 我只回答你自己喂给我的东西。今天想翻哪一页？', sources: ['wiki/identity.md'] },
  ]);
  const [busy, setBusy] = useStateA(false);

  const handleSend = (text) => {
    const next = [...messages, { role: 'user', content: text }];
    setMessages(next);
    setBusy(true);
    // simulate streaming reply
    setTimeout(() => {
      setMessages(m => [...m, {
        role: 'ai',
        streaming: true,
        content: '翻查了 raw/ 最近 7 天、wiki/ 主题簇、graph/ 相邻节点…',
      }]);
    }, 300);
    setTimeout(() => {
      setMessages(m => {
        const copy = [...m];
        copy[copy.length - 1] = {
          role: 'ai',
          content: '你上周最密集的主题是「手术刀 vs 瑞士军刀」 — 在 4 条微信素材和 1 份 PPT 里反复出现。核心观点：放弃通用 AI 客户端的野心，专注把 WeChat → Wiki 这一条漏斗做透。',
          sources: ['raw/2026-W16/3.md', 'wiki/产品哲学.md', 'wiki/CCD-4件套.md'],
        };
        return copy;
      });
      setBusy(false);
    }, 1600);
  };

  return (
    <div className="content narrow fade-in" style={{ display: 'flex', flexDirection: 'column', height: '100%' }}>
      <div style={{ display: 'flex', alignItems: 'baseline', justifyContent: 'space-between', marginBottom: 18 }}>
        <div>
          <div className="h2">Ask</div>
          <div className="muted-txt" style={{ fontSize: 13 }}>Grounded only on your wiki · 1,284 pages indexed</div>
        </div>
        <div style={{ display: 'flex', gap: 8 }}>
          <button className="btn secondary">新会话</button>
          <button className="btn ghost">历史</button>
        </div>
      </div>

      <div style={{ flex: 1, overflowY: 'auto', paddingRight: 4 }}>
        {messages.map((m, i) => <ChatMessage key={i} {...m} />)}
      </div>

      <Composer onSend={handleSend} busy={busy} />
    </div>
  );
}

Object.assign(window, { AskPage });
