/* global React, Link, Check, Clock, ArrowRight, Settings */
const { useState: useConnV2 } = React;

function ConnectPage() {
  const [step, setStep] = useConnV2(1);
  return (
    <div className="kb-page fade-in" style={{ maxWidth: 720 }}>
      <div style={{ marginBottom: 18 }}>
        <div className="section-label">连接外脑 · 微信</div>
        <div className="h1" style={{ marginTop: 6, fontSize: 30 }}>让微信里的内容自动流进来</div>
        <p className="muted-txt" style={{ fontSize: 14.5, marginTop: 10, lineHeight: 1.65 }}>
          用一个专属小号当作"外脑入口"。看到值得收藏的文章、语音、图片，转发给这个小号，ClawWiki 会自动接收、整理并归档。
        </p>
      </div>

      <div className="card" style={{ padding: '8px 22px' }}>
        <div className={`step-row ${step > 1 ? 'done' : step === 1 ? 'active' : ''}`}>
          <div className="n">{step > 1 ? <Check size="3.5" /> : '1'}</div>
          <div style={{ flex: 1 }}>
            <div style={{ fontFamily: 'var(--font-serif)', fontWeight: 500, fontSize: 15.5, color: 'var(--fg-1)' }}>扫码绑定微信小号</div>
            <div className="muted-txt" style={{ fontSize: 13, marginTop: 3 }}>用你的主号扫一下，就能和"外脑小号"建立连接。</div>
            {step === 1 && (
              <div style={{ marginTop: 12, display: 'flex', gap: 14, alignItems: 'center' }}>
                <div style={{ width: 124, height: 124, borderRadius: 10, background: 'var(--pure-white)', border: '1px solid var(--border-cream)', display: 'grid', placeItems: 'center' }}>
                  <QRGraphic />
                </div>
                <div>
                  <div style={{ fontSize: 12.5, color: 'var(--fg-3)', lineHeight: 1.55 }}>打开微信 → 扫一扫<br/>5 分钟内有效</div>
                  <button className="btn primary" style={{ marginTop: 12 }} onClick={() => setStep(2)}>我已扫码 <ArrowRight size="3.5" /></button>
                </div>
              </div>
            )}
          </div>
        </div>
        <div className={`step-row ${step > 2 ? 'done' : step === 2 ? 'active' : ''}`}>
          <div className="n">{step > 2 ? <Check size="3.5" /> : '2'}</div>
          <div style={{ flex: 1 }}>
            <div style={{ fontFamily: 'var(--font-serif)', fontWeight: 500, fontSize: 15.5, color: 'var(--fg-1)' }}>转发一条内容试试</div>
            <div className="muted-txt" style={{ fontSize: 13, marginTop: 3 }}>任何一篇公众号文章、一段语音或一张图都行。</div>
            {step === 2 && (
              <button className="btn primary" style={{ marginTop: 12 }} onClick={() => setStep(3)}>我已转发 <ArrowRight size="3.5" /></button>
            )}
          </div>
        </div>
        <div className={`step-row ${step === 3 ? 'active' : ''}`}>
          <div className="n">3</div>
          <div style={{ flex: 1 }}>
            <div style={{ fontFamily: 'var(--font-serif)', fontWeight: 500, fontSize: 15.5, color: 'var(--fg-1)' }}>看它出现在知识库里</div>
            <div className="muted-txt" style={{ fontSize: 13, marginTop: 3 }}>通常 5 秒内到达。</div>
          </div>
        </div>
      </div>
    </div>
  );
}

function QRGraphic() {
  const cells = [];
  const seed = [0,1,0,1,1,0,0,1,1,0,1,0,1,1,0,1,0,0,1,0,1,1,0,1,1];
  for (let y = 0; y < 9; y++) for (let x = 0; x < 9; x++) {
    cells.push(<rect key={`${x}${y}`} x={x*10} y={y*10} width="9" height="9" fill={seed[(x*7+y*3) % seed.length] ? 'var(--near-black)' : 'transparent'} />);
  }
  return (
    <svg viewBox="0 0 90 90" width="96" height="96">
      <rect x="0" y="0" width="26" height="26" fill="var(--near-black)" />
      <rect x="4" y="4" width="18" height="18" fill="var(--pure-white)" />
      <rect x="8" y="8" width="10" height="10" fill="var(--near-black)" />
      <rect x="64" y="0" width="26" height="26" fill="var(--near-black)" />
      <rect x="68" y="4" width="18" height="18" fill="var(--pure-white)" />
      <rect x="72" y="8" width="10" height="10" fill="var(--near-black)" />
      <rect x="0" y="64" width="26" height="26" fill="var(--near-black)" />
      <rect x="4" y="68" width="18" height="18" fill="var(--pure-white)" />
      <rect x="8" y="72" width="10" height="10" fill="var(--near-black)" />
      {cells}
    </svg>
  );
}

function SettingsPage() {
  const items = [
    { title: '账号', sub: '头像、昵称、微信小号绑定' },
    { title: '整理偏好', sub: '多久整理一次、哪些需要你亲自把关' },
    { title: '存储', sub: '知识库大小、本地/云端位置、导出备份' },
    { title: '外观', sub: '浅色、深色、字号' },
    { title: '快捷键', sub: '查看和自定义常用动作' },
    { title: '关于', sub: 'v2.4.1 · 检查更新 · 反馈' },
  ];
  return (
    <div className="kb-page fade-in" style={{ maxWidth: 720 }}>
      <div className="h1" style={{ fontSize: 30 }}>设置</div>
      <p className="muted-txt" style={{ fontSize: 14, marginTop: 8 }}>
        常用选项在下面。技术细节默认收起。
      </p>
      {items.map((s, i) => (
        <div key={i} className="list-item">
          <div className="l-ico"><Settings size="3.5" /></div>
          <div>
            <div className="l-title">{s.title}</div>
            <div className="l-sub">{s.sub}</div>
          </div>
          <div className="l-meta"><ArrowRight size="3.5" /></div>
        </div>
      ))}
    </div>
  );
}

Object.assign(window, { ConnectPage, SettingsPage });
