/* global React, MessageCircle, BookOpen, FileStack, ArrowRight, Tag */
const { useState: useAskV2 } = React;

function ChatPage() {
  const [messages, setMessages] = useAskV2([
    { role: 'ai', content: '你好。这里是你的知识库助手 —— 我只会用你自己积累下来的内容来回答。今天想聊什么？' },
  ]);
  const [busy, setBusy] = useAskV2(false);
  const [val, setVal] = useAskV2('');

  const send = () => {
    const v = val.trim();
    if (!v || busy) return;
    setMessages(m => [...m, { role: 'user', content: v }]);
    setVal('');
    setBusy(true);
    setTimeout(() => {
      setMessages(m => [...m, {
        role: 'ai',
        content: '上周你最关心的是「手术刀 vs 瑞士军刀」—— 在 4 条微信转发和 1 份 PPT 里都出现过。\n\n核心想法：不做什么都能做一点的通用 AI 客户端，专心把「微信 → 知识库」这一条流程做透。',
        sources: [
          { title: '公众号：手术刀，不是瑞士军刀', when: '4 月 19 日' },
          { title: '周日 PPT · 第 7 页', when: '4 月 19 日' },
          { title: '语音备忘：产品哲学', when: '4 月 18 日' },
        ],
      }]);
      setBusy(false);
    }, 1100);
  };

  return (
    <div className="chat-wrap fade-in">
      <div style={{ flex: 1, overflowY: 'auto', paddingRight: 4, paddingBottom: 12 }}>
        {messages.map((m, i) => (
          <div key={i} className={`chat-msg ${m.role} fade-in`}>
            <div style={{ maxWidth: '82%' }}>
              {m.role === 'ai' && (
                <div style={{ fontSize: 11, color: 'var(--fg-4)', marginBottom: 6, display: 'flex', alignItems: 'center', gap: 6 }}>
                  <span style={{ width: 18, height: 18, borderRadius: 6, background: 'var(--claude-orange)', color: 'var(--ivory)', display: 'grid', placeItems: 'center', fontFamily: 'var(--font-serif)', fontSize: 11, fontWeight: 500 }}>C</span>
                  你的知识库
                </div>
              )}
              <div className="bubble" style={{ whiteSpace: 'pre-wrap' }}>
                {m.content}
                {m.sources && (
                  <div style={{ marginTop: 12, paddingTop: 10, borderTop: '1px solid var(--border-cream)' }}>
                    <div style={{ fontSize: 11, color: 'var(--fg-4)', marginBottom: 6, fontWeight: 500 }}>参考内容 · {m.sources.length}</div>
                    {m.sources.map((s, j) => (
                      <div key={j} style={{ display: 'flex', alignItems: 'center', gap: 8, fontSize: 12.5, padding: '3px 0' }}>
                        <Tag size="3" style={{ color: 'var(--fg-4)' }} />
                        <span style={{ color: 'var(--fg-1)' }}>{s.title}</span>
                        <span style={{ color: 'var(--fg-4)', marginLeft: 'auto' }}>{s.when}</span>
                      </div>
                    ))}
                  </div>
                )}
              </div>
            </div>
          </div>
        ))}
        {busy && (
          <div className="chat-msg ai">
            <div className="bubble" style={{ color: 'var(--fg-3)' }}>
              <span className="streaming-dot"><span className="dot">⏺</span> 正在翻查你最近的内容…</span>
            </div>
          </div>
        )}
      </div>

      <div className="composer">
        <textarea
          value={val}
          onChange={e => setVal(e.target.value)}
          onKeyDown={e => { if (e.key === 'Enter' && (e.metaKey || e.ctrlKey)) { e.preventDefault(); send(); } }}
          placeholder="问一个问题…  按 ⌘↵ 发送"
        />
        <div className="row">
          <span className="chip"><BookOpen size="3" /> 整理好的页面</span>
          <span className="chip"><FileStack size="3" /> 最近 7 天</span>
          <div className="spacer" />
          <span style={{ fontSize: 10.5, color: 'var(--fg-4)' }}>答案只来自你自己的内容</span>
          <button className="btn primary" disabled={busy || !val.trim()} onClick={send}>
            发送 <span style={{ opacity: .7, fontFamily: 'var(--font-mono)' }}>⌘↵</span>
          </button>
        </div>
      </div>
    </div>
  );
}

Object.assign(window, { ChatPage });
