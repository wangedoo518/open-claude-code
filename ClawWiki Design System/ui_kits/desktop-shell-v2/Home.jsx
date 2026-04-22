/* global React, MessageCircle, Wand, BookOpen, Link, ArrowRight, Lightbulb, Plus, GitMerge, Copy, Check, ChevronRight */
const { useState: useHomeV2 } = React;

function SkillCard({ variant, title, sub, emoji, onClick }) {
  return (
    <div className={`skill-card ${variant}`} onClick={onClick}>
      <div className="sc-title">{title}</div>
      <div className="sc-sub">{sub}</div>
      <div className="sc-ill">{emoji}</div>
    </div>
  );
}

function HomePage({ onNavigate }) {
  const [val, setVal] = useHomeV2('');
  const [proposals, setProposals] = useHomeV2([
    { id: 1, kind: 'merge',  title: '《CCD 四件套》和《Claude Code 开发流》要合并吗？', sub: '两页引用的是同一批微信内容' },
    { id: 2, kind: 'new',    title: '「手术刀 vs 瑞士军刀」这个话题出现 4 次了', sub: '建议新建一页，放到"产品哲学"' },
    { id: 3, kind: 'dedupe', title: '两份周记几乎一样', sub: '保留更长的那份，另一份存档' },
  ]);

  const kindIcon = (k) => {
    if (k === 'merge')  return <GitMerge size="4" />;
    if (k === 'new')    return <Plus     size="4" />;
    return <Copy size="4" />;
  };
  const kindText = (k) => ({ merge: '合并', new: '新建', dedupe: '去重' }[k]);

  const skip = (id) => setProposals(p => p.filter(x => x.id !== id));
  const approve = (id) => setProposals(p => p.filter(x => x.id !== id));

  return (
    <div className="fade-in">
      <div className="hero-center">
        <div className="greet">
          Hi，<span className="underline">欢迎回来</span>
          <span className="accent">+</span>
        </div>
        <div className="tagline">随时随地，帮你整理外脑</div>
      </div>

      <div className="skill-row">
        <SkillCard variant="c1" title="问问知识库" sub="答案只来自你自己喂的内容" emoji="💬" onClick={() => onNavigate('chat')} />
        <SkillCard variant="c2" title="待整理 3 条" sub="帮我判断一下该怎么归" emoji="📮" onClick={() => document.getElementById('today-block')?.scrollIntoView({ behavior: 'smooth', block: 'start' })} />
        <SkillCard variant="c3" title="打开知识库" sub="浏览已整理的页面和关系" emoji="📚" onClick={() => onNavigate('kb')} />
        <SkillCard variant="c4" title="连接微信"   sub="让内容自动流进来"       emoji="📱" onClick={() => onNavigate('connect')} />
        <SkillCard variant="c5" title="本周回顾"   sub="看看这周你关心了什么"   emoji="🌏" onClick={() => onNavigate('chat')} />
      </div>

      <div id="today-block" className="today-block">
        <div className="today-head">
          <div className="t-title">今天可以处理</div>
          <div className="t-count">{proposals.length} 条</div>
          <div className="spacer" />
          <div className="mini-link" onClick={() => onNavigate('kb')}>查看全部 <ChevronRight size="3.5" /></div>
        </div>

        {proposals.length === 0 ? (
          <div className="empty" style={{ padding: '30px 20px', background: 'var(--ivory)', borderRadius: 12, border: '1px solid var(--border-cream)' }}>
            <Check size="5" className="ico" style={{ color: 'var(--success)' }} />
            <h4>都处理完了</h4>
            <p>新内容还会陆续到来。</p>
          </div>
        ) : proposals.map(p => (
          <div key={p.id} className={`proposal-card ${p.kind}`}>
            <div className="kind-ico">{kindIcon(p.kind)}</div>
            <div style={{ minWidth: 0 }}>
              <div style={{ display: 'flex', alignItems: 'center', gap: 8, marginBottom: 4 }}>
                <span className="badge muted">{kindText(p.kind)}</span>
              </div>
              <div className="p-title">{p.title}</div>
              <div className="p-sub">{p.sub}</div>
            </div>
            <div className="p-actions">
              <button className="btn ghost" onClick={() => skip(p.id)}>跳过</button>
              <button className="btn primary" onClick={() => approve(p.id)}>按建议处理</button>
            </div>
          </div>
        ))}
      </div>

      <div className="home-composer">
        <div className="home-composer-inner">
          <textarea
            placeholder="想问点什么？ 比如：本周我最关心的话题是什么？"
            value={val}
            onChange={e => setVal(e.target.value)}
            rows={2}
          />
          <div style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
            <span className="chip"><BookOpen size="3" /> 整理好的页面</span>
            <div style={{ flex: 1 }} />
            <button className="btn primary" disabled={!val.trim()} onClick={() => onNavigate('chat')}>
              发送 <ArrowRight size="3.5" />
            </button>
          </div>
        </div>
        <div className="foot">内容由 AI 生成，请仔细甄别</div>
      </div>
    </div>
  );
}

Object.assign(window, { HomePage });
