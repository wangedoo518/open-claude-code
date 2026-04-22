/* global React, ReactDOM, RailV2, TopBarV2, HomePage, ChatPage, KnowledgeBase, ConnectPage, SettingsPage, BookOpen, Network, FileStack */
const { useState: useAppV2 } = React;

function AppV2() {
  const [route, setRoute] = useAppV2('home');
  const [kbTab, setKbTab] = useAppV2('pages');

  let topBar, page;
  if (route === 'kb') {
    const tabs = [
      { id: 'pages', label: '页面',     icon: BookOpen },
      { id: 'graph', label: '关系图',   icon: Network },
      { id: 'raw',   label: '素材库',   icon: FileStack },
    ];
    topBar = <TopBarV2 tabs={tabs} activeTab={kbTab} onTab={setKbTab} />;
    page = <KnowledgeBase tab={kbTab} />;
  } else if (route === 'chat') {
    topBar = <TopBarV2 title="对话" />;
    page = <ChatPage />;
  } else if (route === 'connect') {
    topBar = <TopBarV2 title="微信接入" />;
    page = <ConnectPage />;
  } else if (route === 'settings') {
    topBar = <TopBarV2 title="设置" />;
    page = <SettingsPage />;
  } else {
    topBar = <TopBarV2 title="灵感" />;
    page = <HomePage onNavigate={setRoute} />;
  }

  return (
    <div className="shell">
      <RailV2 route={route} onNavigate={setRoute} reviewCount={3} />
      <div className="main">
        {topBar}
        <div className="content">{page}</div>
      </div>
    </div>
  );
}

const root = ReactDOM.createRoot(document.getElementById('root'));
root.render(<AppV2 />);
