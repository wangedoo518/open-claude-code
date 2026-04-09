# ClawWiki 产品设计方案 · v2（full-CCD + Wiki + WeChat）

> 源起：`Clippings/当知识开始自己生长：Karpathy开源个人LLM Wiki.md`
> 载体：`Warwolf/claudewiki`（`apps/desktop-shell` + `rust/crates/{desktop-core,desktop-server}`）
> 参考源码：`sage-wiki` · `engram` · `defuddle` · `obsidian-importer` · `obsidian-clipper`
>
> **本文档是针对用户第 3 次指令写的新一版**，与同目录下另外两版并存：
>
> - [`product-design-v1.md`](./product-design-v1.md) · v1：判断"不要复刻 CCD"——被用户第 3 次指令明确否决。
> - [`product-design.md`](./product-design.md) · 中间路线版：保留 Ask 页和 chrome 氛围，但主张替换掉"双行 TabBar / `/apps` / `/code`"——与用户第 3 次指令"**要保留 Claude Code Desktop 的复刻交互**"半冲突，作为**对立决策**保留。
> - **本文（product-design-v2.md）= canonical v2**：完整保留 CCD 的双行 TabBar、`/apps`、`/code`、SessionWorkbench，在此之上**加**一个 `Wiki` 一级 Tab 和一整条微信 → Raw → 自动维护的流水线。

---

## 0. 用户第 3 次指令原文 → 我的翻译

> **1. 要保留 Claude Code Desktop 的复刻交互，在此基础上添加 Wiki 的交互功能**
> **2. 一切围绕 ClaudeWiki 笔记去做，我们接微信的主要目的，就是当大家在微信端随便分享一个公众号的文章或者说发一段语音、或者扔一个 PPT 的材料、或者哪怕是上传一个视频，那我们都会把这些作为 Raw 层的数据，然后接着呢，我们这边有 GPT5.4 的 token，海量的 token，帮他去自动化的去处理这个 Wiki**

### 我怎么解读

| 点 | 解读 |
|---|---|
| "保留复刻交互" | 是**完整保留**，不是"保留氛围"。双行 TabBar、`/apps` 画廊、`/code` CLI 启动器、SessionWorkbench 五件套（ContentHeader + MessageItem + InputBar + PermissionDialog + StatusLine + SubagentPanel）一个像素都不拆。中间路线版 `§8.2` 说要替换 TabBar——这条在 v2 canonical 里**被否决**。 |
| "在此基础上添加 Wiki" | 在双行 TabBar 第一行加一个 `Wiki` 一级 Tab；在 HomePage 的 sidebar 加一个 Wiki 分组深链；`/apps` 追加一个内置 MinApp "WeChat Inbox"；`/code` 的 cliTool 列表追加 "Claude Code Desktop" 选项。**全部是加法**。 |
| "一切围绕 ClaudeWiki 笔记" | 产品的**功能重心**是 Wiki。CCD 是**壳**不是**活儿**。用户开 App 不是为了写代码，而是为了看自己被微信喂出来的 wiki。 |
| "接微信是主要目的" | 微信是**主入口**。桌面上不需要"上传"按钮作为主流程——微信丢什么进来，桌面就长什么 wiki。桌面的"上传"保留但退居二线。 |
| "GPT5.4 海量 token" | 维护 agent 默认走 `managed_auth::codex` provider（用户订阅池）。对用户体感是"免费"的自动化。 |

### 开放问题

⚠️ 用户原始第 3 条在 "3." 后被截断。可能是：Obsidian Vault 双向同步 / 团队多用户 / 移动端 / 导出 / 付费分层 / 其它。本文第 9 节有一个"开放问题清单"，等你补充后我再加。

---

## 1. TL;DR（120 秒读完）

```
          ┌────────────────────────────────────────────────────────┐
          │  Claude Code Desktop chrome（双行 TabBar + Row2 sessions）│
          │                                                        │
          │    首页 │ 应用 │ [Wiki ← NEW] │ Code │ 设置              │
          │                                                        │
          │  ┌──────┐   ┌────────────────────────────────────┐     │
          │  │ /home│   │   当前选中的 Tab 内容（同 CCD）    │     │
          │  │ side │   │                                    │     │
          │  │ bar  │   │   /wiki/* 子路由用 DeepTutor 暖色   │     │
          │  │      │   │   chrome→content 切换感            │     │
          │  │ Wiki │   │                                    │     │
          │  │(NEW) │   │   其余 /home /apps /code /settings │     │
          │  │      │   │   视觉保持原 CCD 灰黑蓝紫          │     │
          │  │ Sess │   │                                    │     │
          │  │ ...  │   │                                    │     │
          │  └──────┘   └────────────────────────────────────┘     │
          │                                                        │
          │  SessionWorkbench 五件套零改动                          │
          │  └─ ContentHeader 多一个 [Code ▾ / Wiki ▾] mode 下拉    │
          │  └─ 切到 Wiki 时换工具集，其它组件不动                  │
          └────────────────────────────────────────────────────────┘
                 │                                  ▲
                 │                                  │
                 ▼                                  │
     ┌──────────────────────────┐         ┌────────────────────┐
     │   WeChat 主入口          │         │  Token Broker       │
     │                          │         │  127.0.0.1:4357/v1  │
     │  企业微信外联机器人       │         │  供给 Ask / 维护器 /│
     │      │                   │         │  外部 CCD / Cursor  │
     │      ▼                   │         │                     │
     │   wechat-ingest svc      │         │  账号池 =           │
     │      │                   │         │  managed_auth::     │
     │   WS 推送→桌面            │         │    codex            │
     │      │                   │         │  + CloudManaged src │
     │      ▼                   │         │    (trade 下发)     │
     │  前端 pipeline:          │◀────────┤                     │
     │  defuddle + clipper/api  │         │  GPT-5.4 海量 token │
     │      │                   │         │                     │
     │      ▼                   │         └─────────────────────┘
     │  POST /api/wiki/raw/     │
     │  ingest → ~/.clawwiki/raw│
     │      │                   │
     │      ▼                   │
     │  wiki-maintainer 自动     │
     │  触发 Wiki mode session   │
     │  → 写 wiki/ + log.md      │
     └──────────────────────────┘
```

10 个决策一句话版：

1. **D1 保留 CCD**：TabBar/sessions/apps/code 全留。只**加**一个 `Wiki` Tab 和 8 个子路由。
2. **D2 SessionWorkbench 不分叉**：加 mode 下拉（Code/Wiki），工具集由 mode 决定。
3. **D3 数据层**：`~/.clawwiki/{raw,wiki,schema}` + manifest.json + `git init` 白送版本历史。
4. **D4 微信是主入口**：企业微信外联机器人为主，个人微信桥接为 opt-in。
5. **D5 HTML→MD 用 defuddle + obsidian-clipper/api**：两个 npm 包在 Tauri WebView 里直接跑；**不用** obsidian-importer（硬绑 Obsidian Vault API）。
6. **D6 WeChat 专属 extractor**：fork defuddle 加 `src/extractors/wechat.ts`（~200 行，照抄 `substack.ts`）。
7. **D7 维护 LLM**：GPT-5.4 via `managed_auth::codex`。MVP 抄 engram 单次调用，规模化抄 sage-wiki 5-pass。
8. **D8 Schema 层必须先于代码**：`schema/CLAUDE.md` 初版定死纪律。
9. **D9 Token Broker 继承 v1**：把 `docs/desktop-shell/cloud-managed-integration.md` 这次做完；外部 CCD/Cursor/claw-cli 通过本机 Broker 复用同一池 Codex。
10. **D10 视觉**：CCD chrome 保留冷色；`/wiki/*` 内容区用 DeepTutor 暖色（`#FAF9F6` bg + `#C35A2C` 烧陶橙 + Lora 衬线）——形成"进入笔记空间"的切换感。

---

## 2. 五份源码研究结论（比前两版更深）

### 2.1 Karpathy gist（`https://gist.github.com/karpathy/442a6bf555914893e9891c11519de94f`）

实际抓取的要点：

- **三层**：`raw/`（只读事实）/ `wiki/`（LLM 主笔 markdown）/ **schema**（`CLAUDE.md` 或 `AGENTS.md`）。
- **Schema 决定纪律**：原话 "it's what makes the LLM a disciplined wiki maintainer rather than a generic chatbot"。
- **一次 ingest 触发 10-15 页更新**：原话 "reads the source, discusses key takeaways with you, writes a summary page, updates the index, updates relevant entity and concept pages across the wiki"。
- **特殊文件协议**：
  - `index.md`：按分类列每个页面 + 一句话摘要 + 元数据，每次 ingest 重建
  - `log.md`：append-only，格式 `## [YYYY-MM-DD] ingest | Article Title`，原话 "the log becomes parseable with simple unix tools"
- **Query 不是一次性问答**：LLM 先读 index 再 drill down，答案可以被 filed 回 wiki 作为新页 —— "explorations compound rather than disappear into chat history"。
- **5 件 Lint**：contradictions / stale claims / orphan pages / missing cross-references / missing concept pages。
- **金句**："Obsidian is the IDE; the LLM is the programmer; the wiki is the codebase."
- **模块化声明**："Everything mentioned above is optional and modular — pick what's useful, ignore what isn't."

**偷**：三层结构、`index.md`+`log.md` 文件协议、5 件 lint、人/LLM 分工语言。

### 2.2 sage-wiki（Go · 工程化最重的参考）

`/Users/champion/Documents/develop/Warwolf/sage-wiki`

关键 insight：把 Karpathy 方案做成了 **5-pass compiler** + **hybrid search** + **MCP server** 的全套工程化实现。

**3 层物理结构**：
- `raw/`（多目录、支持 `.md/.pdf/.docx/.xlsx/.pptx/.epub/.eml/.txt/.vtt/.srt/image/code`）
- `wiki/summaries/*.md` + `wiki/concepts/*.md`
- `.sage/wiki.db` 是 SQLite（FTS5 + 向量 `vec_entries` + `entities`/`relations` 图）
- `.manifest.json` 追踪每个 source 的 hash/status/concepts

**5-pass pipeline**（`internal/compiler/pipeline.go`）：

| Pass | 文件 | 动作 | LLM 调用次数 | 关键优化 |
|---|---|---|---|---|
| 0 Diff | `internal/compiler/diff.go` | hash 对比 manifest，产出 Added/Modified/Removed | 0 | 只处理增改 |
| 1 Summarize | `internal/compiler/summarize.go` | 每源一次 LLM | 1/源 | prompt cache |
| 2 Extract Concepts | `internal/compiler/concepts.go` | 每 20 摘要一次 LLM，JSON 去重 | 1/20 源 | batch |
| 3 Write Articles | `internal/compiler/write.go` | 每 concept 一次 LLM，生成 `[[wikilinks]]` | 1/concept | prompt cache |
| 4 Images | `internal/compiler/images.go` | Vision LLM caption | 1/图 | optional |

**成本控制**：prompt cache 省 50-90% 输入 token + Anthropic/OpenAI **Batch API** 再省 50%（`internal/llm/batch.go`）+ `CompileState` 断点续传。

**Hybrid 检索**：BM25 (FTS5) + 向量余弦 → **Reciprocal Rank Fusion**（`internal/hybrid/search.go`，`k=60`）。

**Ontology**：`entities` 限 5 类型 (concept/technique/source/claim/artifact) + `relations` 限 8 关系 (implements/extends/optimizes/cites/prerequisite_of/trades_off/derived_from/contradicts)。**类型封顶是防图爆炸的关键**。

**MCP**：`internal/mcp/server.go` 暴露 15 个工具（5 读 + 8 写 + 2 复合）+ stdio/SSE 双 transport。**复合工具 `wiki_capture` 最有价值**：从对话里抽知识回 wiki。

**Lint passes**（`internal/linter/passes.go`）：completeness / style / consistency / learning + `staleness_threshold_days: 90`。

**偷**：整个 5-pass 骨架 · prompt cache + Batch API + checkpoint · manifest tracker · MCP 15 工具分区 · RRF · ontology 类型封顶 · `wiki_capture` 思路。
**不偷**：Go 栈、嵌 Preact（我们已有 desktop-shell）、自造 LLM provider 抽象（复用 `managed_auth`）。

### 2.3 engram（Python · MVP 最小可行）

`/Users/champion/Documents/develop/Warwolf/engram`

- 10 个 Typer 命令：`init / save / ingest / query / lint / compress / status / export / forget / topics / version`
- 架构：`~/.engram/` 三层 —— `sources/raw/NNNN_slug.md` + `wiki/*.md` + `engram.toml`
- **Ingest 核心**（`core/ingest.py`）一次 LLM 调用搞定：
  1. Fetch 源 → md（URL: BS4+markdownify；PDF: pypdf）
  2. 落 `sources/raw/NNNN_title.md`（顺序编号、不可变）
  3. 读当前 `wiki/index.md` + 相关页
  4. 一次 LLM，返回 JSON 数组 `[{action, slug, title, content, tags, summary}]`
  5. **Pydantic 严格校验** 后写盘、`append_log`、`_rebuild_index`
- **Lint**（`core/lint.py`）：把全部文章喂给 LLM，让它报 `contradiction/stale/missing_xref/stub/orphan/duplicate`
- **Compress**（防爆）：按 tag 合并"热"条目
- **安全**：SSRF 白名单 + 私网 IP 拦截 + slug path traversal 拦截
- **缺**：MCP、自动 backlink、冲突合并、全文检索、UI

**偷**：Pydantic 严格校验 LLM 返回、`_rebuild_index()`+`append_log()` 协议、"单次 LLM 调用"MVP 范式、SSRF 防护、`compress` 防爆。

### 2.4 defuddle（TS · HTML → 干净 MD · MIT）

`/Users/champion/Documents/develop/Warwolf/defuddle`

API：

```ts
// 浏览器（同步）
const result = new Defuddle(document, { url: document.URL }).parse();

// Node（异步）
const result = await Defuddle(doc, url, options);
```

返回：`{ content (cleaned HTML), contentMarkdown?, title, author, published, site, description, image, favicon, domain, language, schemaOrgData, wordCount, metaTags, ... }`

- `defuddle/full` 打进 Turndown + math 做 HTML → MD
- `src/extractors/` 已内置：substack / reddit / twitter / youtube / github / chatgpt / claude / grok / gemini / hn / c2wiki / bbcode
- **没有** `wechat.ts` / `mp.weixin.qq.com` 提取器

我们要 fork 加一个 `src/extractors/wechat.ts`（骨架~200 行，照抄 `substack.ts`）：

```ts
// src/extractors/wechat.ts
import { BaseExtractor, ExtractorResult } from "./_base";

export class WeChatExtractor extends BaseExtractor {
  static matches(url: URL) {
    return url.hostname === "mp.weixin.qq.com";
  }

  extract(doc: Document, url: string): ExtractorResult {
    // 1. 懒加载图：data-src → src
    const body = doc.getElementById("js_content");
    body?.querySelectorAll("img[data-src]").forEach((img) => {
      const src = img.getAttribute("data-src");
      if (src) img.setAttribute("src", src);
      img.setAttribute("referrerpolicy", "no-referrer");
    });

    // 2. 视频/小程序 placeholder 替换成静态链接
    body?.querySelectorAll("iframe[data-src]").forEach((iframe) => {
      const link = doc.createElement("a");
      link.href = iframe.getAttribute("data-src") ?? "";
      link.textContent = `[视频: ${link.href}]`;
      iframe.replaceWith(link);
    });

    // 3. 元数据
    const title = doc.getElementById("activity-name")?.textContent?.trim()
                 ?? doc.querySelector("meta[property='og:title']")?.getAttribute("content")
                 ?? undefined;
    const author = doc.getElementById("js_name")?.textContent?.trim()
                 ?? doc.querySelector("meta[name='author']")?.getAttribute("content")
                 ?? undefined;
    const publishedRaw = doc.getElementById("publish_time")?.textContent?.trim();
    const published = publishedRaw ? new Date(publishedRaw).toISOString() : undefined;
    const description = doc.querySelector("meta[name='description']")?.getAttribute("content") ?? undefined;
    const image = doc.querySelector("meta[property='og:image']")?.getAttribute("content") ?? undefined;

    return {
      contentSelector: "#js_content",
      content: body?.outerHTML ?? "",
      title,
      author,
      published,
      description,
      image,
      site: "微信公众号",
    };
  }
}
```

注册到 `src/extractor-registry.ts` 的 `extractors` 数组头部即可。

### 2.5 obsidian-importer（TS · **决定不用**）

`/Users/champion/Documents/develop/Warwolf/obsidian-importer`

支持 13 种格式（Notion / Evernote / Apple Notes / Bear / Google Keep / OneNote / Roam / HTML / CSV / Tomboy...），但**每个 `format/*.ts` 都直调 `app.vault.createBinary()` / `app.fileManager.processFrontMatter()`**——硬绑 Obsidian Vault API。

- 没 WeChat
- 没 PPT 原生解析
- 没音频转写

抽出来重构成本 > 重写成本。**直接跳过**。PPT/DOCX 走 `python-pptx`/`mammoth`（Rust spawn），PDF 走 `pdfjs-dist` + `pypdf`，音频走 Whisper。

### 2.6 obsidian-clipper（TS · **关键发现：`api.ts` 环境无关**）

`/Users/champion/Documents/develop/Warwolf/obsidian-clipper`

**`src/api.ts` 是完全环境无关的**——0 个 `chrome.*`、0 个 `browser.*`、0 个 DOM 依赖——已经把 defuddle 串好了。签名：

```ts
// src/api.ts:176
export async function clip(options: ClipOptions): Promise<ClipResult>;

interface ClipOptions {
  html: string;
  url: string;
  template: Template;              // noteNameFormat + noteContentFormat + properties[]
  documentParser: DocumentParser;  // { parseFromString(html, mime) }
  propertyTypes?: Record<string, string>;
  parsedDocument?: any;
}

interface ClipResult {
  noteName: string;     // 应用完模板的文件名
  frontmatter: string;  // YAML 字符串
  content: string;      // markdown 正文
  fullContent: string;  // frontmatter + content
  properties: Property[];
  variables: Record<string, string>;
}
```

内部：

```
defuddle.parse()
  → createMarkdownContent()    # 用 Turndown 把 content HTML 变 markdown
    → buildVariables()          # 聚合 title/author/... 给模板
      → compileTemplate()       # 模板引擎 {{title}} {{author}} {{published | date_format}} ...
        → sanitizeFileName()    # 文件名安全化
          → generateFrontmatter()
```

模板系统支持 `{{title}}` `{{author}}` `{{published | date_format("YYYY-MM-DD")}}` `{{content}}` `{{selector:"#js_content"}}` + 30+ filter（upper/lower/slice/replace/date_modify/reverse/...）。

**结论：直接用**。前端装 `defuddle`（我们 fork 的，带 wechat extractor）+ `obsidian-clipper`（用 `/api` 子路径），`documentParser` 用 Tauri WebView 原生 `DOMParser`，一步出 md + frontmatter，`fetch('http://127.0.0.1:4357/api/wiki/raw/ingest')` 给 Rust 落盘。

### 2.7 三选一总结

| 库 | 决策 | 作用 |
|---|---|---|
| **defuddle** | 用 | HTML → 干净 HTML/Markdown + 元数据抽取；fork 加 WeChat extractor |
| **obsidian-clipper** | 用（只用 `/api`） | defuddle 之上的模板引擎 + YAML frontmatter + filter 管线 |
| **obsidian-importer** | 不用 | Obsidian Vault API 硬绑，重构代价 > 重写 |

---

## 3. 核心决策（D1–D10）

### D1 · 保留 Claude Code Desktop 的**全部**复刻

相较中间路线版 `§8.2` 要"替换掉顶部双层 TabBar、`/apps`、`/code`"，v2 canonical **反转**这一点：

| 组件 | 动作 | 依据 |
|---|---|---|
| `shell/AppShell.tsx` | **仅新增路由**，不改现有 | 保留 `/home` `/apps` `/apps/:id` `/code` |
| `shell/TabBar.tsx` | **仅新增一个 TabItem** 指向 `/wiki` | `首页\|应用\|Wiki(NEW)\|Code\|设置` |
| `shell/TabItem.tsx` | **零改动** | |
| TabBar Row 2 session tabs | **零改动** | 会话切换逻辑原样 |
| `features/workbench/HomePage.tsx` | 只在 sidebar 的 PRIMARY_ITEMS 后面加一条 Wiki 分组链接，**布局零改动** | |
| `features/session-workbench/*` | **整体零改动** + 接受 `mode: "code"\|"wiki"` prop | 工具集由 mode 决定 |
| `ContentHeader.tsx` | **加一个 mode 下拉** | Code/Wiki 二选一 |
| `PermissionDialog.tsx` | **零改动** | low/medium/high 分级对 wiki 工具同样适用 |
| `StatusLine.tsx` | **零改动** + 在右侧显示 `mode: wiki` + `provider: codex/gpt-5.4` | |
| `features/apps/*`（MinApps 画廊） | **保留** + 追加一个内置 MinApp `WeChatInbox` | tray-style |
| `features/code-tools/CodeToolsPage.tsx`, `/code` | **保留** + 在 cliTool 下拉加 "Claude Code Desktop" 选项 | 选中 runCodeTool 时注入 Broker env |
| `features/auth/*` | **零改动** | |
| `features/billing/*` | **保留** + `cloud-accounts-sync.ts` 改走 Rust endpoint（v1 规划这次做完） | |
| `features/settings/*` | **保留** + 追加 `TokenBrokerSettings` / `WeChatBridgeSettings` / `WikiStorageSettings` 三个 section | |

> 打开 ClawWiki，第一眼看到的还是那个双行 TabBar、traffic lights、session tab 条——只是多了一个 `Wiki` 按钮。CCD 肌肉记忆一点都不丢。

### D2 · `/wiki` 一级路由 + 8 条子路由

```tsx
// apps/desktop-shell/src/shell/AppShell.tsx 新增
<Route path="/wiki"             element={<WikiHomePage />} />
<Route path="/wiki/raw"         element={<RawLibraryPage />} />
<Route path="/wiki/pages"       element={<WikiExplorerPage />} />
<Route path="/wiki/pages/:slug" element={<WikiPageDetail />} />
<Route path="/wiki/graph"       element={<GraphPage />} />
<Route path="/wiki/schema"      element={<SchemaEditorPage />} />
<Route path="/wiki/inbox"       element={<WikiInboxPage />} />
<Route path="/wiki/wechat"      element={<WeChatBridgePage />} />
```

每个子路由用 `PageTransition` fade-in 包裹；TabBar 的 `Wiki` TabItem 指向 `/wiki`。

### D3 · 数据层 `~/.clawwiki/`

```
~/.clawwiki/
├── raw/                              # 不可变事实层
│   ├── 00001_wechat_karpathy-llm-wiki_2026-04-08.md
│   ├── 00002_wechat_voice_2026-04-08_duration60s.md
│   ├── 00003_wechat_pptx_2026-04-08_slug.md
│   └── attachments/
│       ├── 00001/cover.jpg
│       └── 00003/slide-01.png
│
├── wiki/
│   ├── index.md                      # Maintainer 自动重建
│   ├── log.md                        # append-only `## [YYYY-MM-DD HH:MM] ingest | ...`
│   ├── concepts/*.md
│   ├── people/*.md
│   ├── topics/*.md
│   ├── compare/*.md
│   └── changelog/YYYY-MM-DD.md
│
├── schema/
│   ├── CLAUDE.md                     # wiki-maintainer 系统提示
│   ├── AGENTS.md                     # 多 agent 分工
│   ├── templates/
│   │   ├── concept.md
│   │   ├── people.md
│   │   ├── topic.md
│   │   ├── compare.md
│   │   └── wechat-clip.clipper.json  # obsidian-clipper Template
│   └── policies/
│       ├── maintenance.md            # 5 件维护动作硬规则
│       ├── conflict.md
│       ├── deprecation.md
│       └── naming.md
│
├── .clawwiki/
│   ├── manifest.json                 # 抄 sage-wiki
│   ├── compile-state.json            # 抄 sage-wiki，断点续传
│   └── wiki.db                       # MVP 不要；规模化时加 FTS5/vector
│
└── .git/                             # git init，白送版本历史
```

MVP 方针：**全是 markdown + 一个 manifest.json**（抄 engram）。规模化到 2k+ 页时再加 FTS5 + 向量 + ontology（抄 sage-wiki）。

### D4 · 微信是主入口

#### 组件边界

```
微信侧                              云侧                             桌面侧
┌─────────────────┐                 ┌───────────────────┐           ┌─────────────────────┐
│ 企业微信外联机器│                 │  wechat-ingest    │           │  desktop-shell      │
│ 人（主推）      │──── webhook ───▶│  新服务 :8904     │           │                     │
│                 │                 │                   │           │  /wiki/wechat 页面  │
│ 公众号订阅号    │                 │  - 企微签名校验   │           │  + /apps/wechat-    │
│ （被动回复）    │                 │  - 分用户 / JWT   │           │    inbox MinApp     │
│                 │                 │  - 附件转对象存储 │           │                     │
│ 个人微信桥接    │                 │  - 只中继不入库   │           │  WS /ws/wechat-inbox│
│ （opt-in 不合规）│                 │  - 30 天 TTL      │──── WS ──▶│  订阅 → tray 事件   │
└─────────────────┘                 │                   │           │                     │
                                    │  POST /api/v1/    │           │  收到 blob_url:     │
                                    │   wechat/webhook  │           │   → 本机下载        │
                                    │  GET  /blob/:id   │           │   → 前端 pipeline    │
                                    │  WS   /ws/wechat- │           │   → Rust 落盘        │
                                    │       inbox       │           │   → 触发 maintainer │
                                    └───────────────────┘           └─────────────────────┘

🔒 原文永不经第三方 LLM——维护 LLM 是用户自己订阅的 Codex GPT-5.4
🔒 云侧只中继不入库；blob 用 AES-GCM + 短时签名 URL
```

#### 接入方式优先级

| 方式 | 合规 | 能接收的消息类型 | 推荐 |
|---|---|---|---|
| **企业微信外部联系人机器人** | ✅ 完全合规，官方 API | 文本/图/语音/视频/文件/链接/小程序卡片/聊天记录 **全支持** | ⭐⭐⭐⭐⭐ 主推 |
| **公众号订阅号 + 被动回复** | ✅ 完全合规 | 只能接收用户主动发的消息，链接要用户"收藏"到机器人 | ⭐⭐⭐ 次选 |
| **个人微信桥接**（`wechaty`/`itchat`） | ❌ 封号风险 | 全支持 | ⭐ opt-in 高级功能，红字警告 |

#### 各类素材 → Raw 层映射

| 微信输入 | 云侧动作 | 桌面侧 pipeline | Raw 输出 |
|---|---|---|---|
| 文本 | 直接转发 | 包成 md | `00NNN_wechat_text_{slug}.md` |
| **mp.weixin.qq.com URL** | 只转发 URL，不 fetch | 桌面 fetch → fork defuddle（wechat extractor）→ `clipper/api::clip(template=wechat-clip)` → md + frontmatter | `00NNN_wechat_article_{pub}_{slug}.md` + `attachments/` |
| 普通 URL | 只转发 URL | 同上，不经 wechat 分支，走 defuddle 通用链路 | `00NNN_wechat_url_{slug}.md` |
| 语音 `.amr`/`.mp3`/`.silk` | 转对象存储 | 下载 → Rust ffmpeg 转 mp3 → `whisper.cpp`（本地）或 Whisper API → md | `00NNN_wechat_voice_{dur}.md` |
| 图片 `.jpg` | 转对象存储 | 下载 → 本机 Broker `/v1/chat/completions` 走 GPT-5.4 Vision caption + OCR → md | `00NNN_wechat_image_{sha}.md` + 原图 |
| **PPT `.pptx`** | 转对象存储 | 下载 → Rust spawn `python-pptx` → 每 slide 一个 `# Slide N` + 备注 + 图 | `00NNN_wechat_pptx_{slug}.md` + slide 图 |
| PDF | 转对象存储 | 下载 → 前端 `pdfjs-dist` 抽文本 + 页边图 | `00NNN_wechat_pdf_{slug}.md` |
| DOCX | 转对象存储 | 下载 → Rust spawn `mammoth`（Node）转 HTML → defuddle 通用链路 | `00NNN_wechat_docx_{slug}.md` |
| **视频 `.mp4`** | 转对象存储 | 下载 → Rust ffmpeg 抽音轨 + 每 10s 抽帧 → Whisper 音轨 + 关键帧 Vision caption → md | `00NNN_wechat_video_{dur}.md` + `attachments/frames/` |
| 小程序卡片 | 解析 JSON | 尝试反推落地 URL → 走 URL pipeline；失败留 JSON | `00NNN_wechat_card_{appid}.md` |
| 聊天记录片段 | 解析 JSON | 按发言人聚合 + 去噪 + 主题分段 → md 列表 | `00NNN_wechat_chat_{count}.md` |

#### 前端 pipeline 代码布局

```
apps/desktop-shell/src/features/wiki/ingest/
├── pipeline.ts                # 按 kind 分派
├── adapters/
│   ├── text.ts
│   ├── html.ts                # 通用 HTML
│   ├── wechat-article.ts      # mp.weixin 专用（forked defuddle）
│   ├── voice.ts               # 调 Rust /api/wiki/ingest/voice
│   ├── image.ts               # 调本机 Broker /v1 vision
│   ├── pdf.ts                 # pdfjs-dist
│   ├── pptx.ts                # 调 Rust /api/wiki/ingest/pptx
│   ├── docx.ts                # 调 Rust /api/wiki/ingest/docx
│   ├── video.ts               # 调 Rust /api/wiki/ingest/video
│   ├── card.ts                # 小程序卡片
│   └── chat.ts                # 聊天记录
├── templates/
│   └── wechat-clip.json       # clipper Template
└── persist.ts                 # POST /api/wiki/raw/ingest
```

#### `adapters/wechat-article.ts` 核心

```ts
import DefuddleClass from "@clawwiki/defuddle-fork";      // 带 wechat extractor
import { clip } from "obsidian-clipper/api";
import wechatTemplate from "../templates/wechat-clip.json";
import { persistRaw } from "../persist";

export async function ingestWeChatArticle(url: string): Promise<{ rawId: string }> {
  // 1. fetch HTML（WebView 有 CORS，所以走 Rust 代理）
  const html = await fetch(`/api/wiki/fetch?url=${encodeURIComponent(url)}`)
    .then(r => r.text());

  // 2. WebView 原生 DOMParser 作 documentParser
  const documentParser = {
    parseFromString: (h: string, m: string) =>
      new DOMParser().parseFromString(h, m as DOMParserSupportedType),
  };

  // 3. clipper 一步走完 defuddle → markdown → template → frontmatter
  const result = await clip({
    html,
    url,
    template: wechatTemplate,
    documentParser,
  });

  // 4. 异步下载图片 attachments
  const attachments = await downloadImagesFromMarkdown(result.content, url);

  // 5. 落盘（通过 Rust endpoint）
  const { rawId } = await persistRaw({
    kind: "wechat-article",
    sourceUrl: url,
    title: result.noteName,
    markdown: result.fullContent,       // YAML frontmatter + markdown
    attachments,
  });

  return { rawId };
}
```

#### `templates/wechat-clip.json`

```json
{
  "name": "WeChat Article",
  "noteNameFormat": "{{published | date_format(\"YYYYMMDD\")}}-{{title | slugify}}",
  "noteContentFormat": "# {{title}}\n\n> {{description}}\n\n{{content}}",
  "properties": [
    { "name": "source",      "value": "wechat",                                          "type": "text" },
    { "name": "source_url",  "value": "{{url}}",                                          "type": "text" },
    { "name": "author",      "value": "{{author}}",                                       "type": "text" },
    { "name": "site",        "value": "{{site}}",                                         "type": "text" },
    { "name": "published",   "value": "{{published | date_format(\"YYYY-MM-DD\")}}",      "type": "date" },
    { "name": "ingested_at", "value": "{{now | date_format(\"YYYY-MM-DD HH:mm:ss\")}}",   "type": "datetime" },
    { "name": "schema",      "value": "v1",                                               "type": "text" },
    { "name": "type",        "value": "raw",                                              "type": "text" },
    { "name": "status",      "value": "ingested",                                         "type": "text" }
  ]
}
```

### D5 · 自动维护 · MVP 抄 engram，规模化抄 sage-wiki

#### MVP（Sprint 3，engram 风格 · 一次 LLM 调用）

触发：`desktop-server` 收到 `POST /api/wiki/raw/ingest` 后发 `wiki_ingest_event`。

`rust/crates/wiki-maintainer` 监听 → 起一个新的 `SessionWorkbench` 会话（后台、不 UI 暴露，但用户在 `/wiki/inbox` 能看到进度）：

- mode = wiki
- provider = codex/gpt-5.4
- 系统提示 = `schema/CLAUDE.md`
- 上下文 = 新 raw 内容 + `wiki/index.md` + 与 raw tag 相关的现有页 + `log.md` 尾 20 行

一次 LLM 调用返回 JSON：

```json
{
  "actions": [
    { "tool": "write_page",      "slug": "concept/llm-wiki",           "body": "---\ntype: concept\n..." },
    { "tool": "patch_page",      "slug": "compare/rag-vs-llm-wiki",    "diff": "..." },
    { "tool": "link_pages",      "src": "concept/llm-wiki", "dst": "people/karpathy", "rel": "authored_by" },
    { "tool": "link_pages",      "src": "concept/llm-wiki", "dst": "concept/rag",     "rel": "contrasts_with" },
    { "tool": "touch_changelog", "entry": "## [2026-04-09 14:22] ingest | LLM Wiki article" },
    { "tool": "rebuild_index",   "scope": "full" }
  ]
}
```

Rust 侧用 `serde` + `validator` 严格校验，每个 action 过 `PermissionDialog`（首次询问，"always allow wiki mode" 后静默），执行 → 写盘 → Inbox 通知。

#### 规模化（v2+，sage-wiki 风格 · 5-pass compiler）

触发条件：wiki 页 > 500 或 raw 入库 > 50/天。

| Pass | Input | LLM 调用 | 优化 |
|---|---|---|---|
| Diff | raw/ + manifest | 0 | |
| Summarize | new sources | 1/源 | prompt cache |
| Extract Concepts | ≥ 20 summaries 积压 | 1/20 源 | batch |
| Write Articles | new concepts | 1/concept | prompt cache |
| Images | image refs | 1/图 | optional |

+ Anthropic/OpenAI Batch API 再省 50%
+ `CompileState` 存 `.clawwiki/compile-state.json`，断点续传

### D6 · LLM 供给：Codex (GPT-5.4) 走本地 Broker

继承 v1 设计，把 v1 规划好但没落地的 Rust 改动这次做完：

1. **`managed_auth.rs`** 新增 `CloudManaged` source + `import_cloud_accounts` / `list_cloud_accounts` / `clear_cloud_accounts` + `delete_codex_profile` 对 CloudManaged 拒绝操作
2. **`desktop-server`** 新增 `/api/desktop/cloud/codex-accounts/{sync,list,clear}`
3. **`desktop-server`** 新增 **本机 Broker**：
   - `POST /v1/chat/completions` （OpenAI 兼容 → 转发到 Codex）
   - `POST /v1/messages` （Anthropic 兼容 → 转发到 Codex）
   - `GET  /v1/models` （聚合 provider 目录）
   - `GET  /api/broker/status` （健康/配额/刷新时间）
   - `POST /api/broker/launch-client` （spawn 外部 CCD，等价旧 runCodeTool，但 cliTool 固定为 claude-code，注入 `ANTHROPIC_BASE_URL=http://127.0.0.1:4357/v1` + `ANTHROPIC_AUTH_TOKEN=<broker short-lived jwt>`）

Broker 的消费者：
1. ClawWiki Ask 会话（Code mode + Wiki mode）
2. ClawWiki wiki-maintainer 后台自动
3. 外部 Claude Code Desktop / Cursor / claw-cli

Broker 只绑 127.0.0.1。access/refresh token 只存 OS keychain（macOS Keychain / Windows Credential Manager / Linux Secret Service）。退订时 `clear_cloud_accounts` 清 registry，Broker 下一轮 401。

### D7 · Schema v1 · `schema/CLAUDE.md` 初稿（必须先于代码）

```markdown
# CLAUDE.md · wiki-maintainer agent rules

## Role
You are the wiki-maintainer for ClawWiki, running in Wiki mode under
SessionWorkbench. Human curates sources (mostly from WeChat); you maintain
pages. Never invert this responsibility.

## Layer contract
- raw/     read-only, never mutate, each file has unique sha256
- wiki/    you write, must pass Schema v1 frontmatter validation
- schema/  human-only; you may PROPOSE changes via Inbox, never write directly

## Triggers
Every `raw_ingested(source_id)` event MUST fire the 5 maintenance actions:
  1. summarise the new source (≤ 200 words, original wording; quote ≤ 15 words)
  2. update affected concept / people / topic / compare pages (create if absent)
  3. add / update backlinks (bidirectional: A→B implies B→A)
  4. detect conflicts with existing judgements → `mark_conflict` → Inbox
  5. append to `changelog/YYYY-MM-DD.md`: `## [HH:MM] ingest | {title}`
     and append to `log.md`

After all actions call `rebuild_index` once to refresh wiki/index.md.

## Frontmatter (schema v1, required)
type:          concept | people | topic | compare | changelog | raw
status:        canonical | draft | stale | deprecated | ingested
owner:         user | maintainer
schema:        v1
source:        wechat | upload | url | ask-session
source_url:    (when applicable)
published:     ISO-8601 date (for raw articles)
ingested_at:   ISO-8601 datetime
last_verified: ISO-8601 date

## Tool permissions (PermissionDialog enforces)
low    : read_source · read_page · search_wiki · rebuild_index
medium : write_page · patch_page · link_pages · touch_changelog
high   : ingest_source · deprecate_page · mark_conflict

## Never do
- Never rewrite raw/ files
- Never silently merge conflicting pages — always mark_conflict
- Never deprecate a page without a replacement slug
- Never summarise in > 200 words
- Never quote > 15 consecutive words from raw/ (copyright safety)
- Never emit backlinks to non-existent pages (link_pages must precheck)
- Never touch schema/ — propose via Inbox instead

## When uncertain
Use `mark_conflict` with reason="uncertain: ${reason}" and move on.
The human will triage in Inbox.
```

### D8 · 视觉：壳冷 · 内容暖

- `/home`、`/apps`、`/apps/:id`、`/code`、`/settings` 保持**现有 CCD 视觉**（灰黑 + 蓝紫、Geist/Inter 等无衬线、密集信息栏）
- `/wiki/*` 切**DeepTutor 暖色**：
  - `--background: #FAF9F6` / `--foreground: #2D2B28`
  - `--primary: #C35A2C` 烧陶橙
  - `--card: #FFFFFF` / `--border: #E8E4DE`
  - Lora 衬线用于正文（Wiki Page Detail 是主要受益者）
  - Plus Jakarta Sans 用于 UI chrome
- 通过 `@container` 或顶层 `data-scope="wiki"` 选择器在 CSS 变量层面切换，不影响其它路由
- **目的**：让"进入笔记空间"有明确的切换感——和 v1 的"全应用用暖色"以及 middle-path 的"纯暖色"都不同，v2 canonical 是**一冷一暖双主题**

### D9 · SessionWorkbench mode 下拉

在 `ContentHeader.tsx` 的右侧加一个小下拉：

```
[ Opus 4.6 ▾ ]  [ mode: Code ▾ ]  [ Local ▾ ]  ...
                         └─ Code
                            Wiki   ← 切换后重新协商工具集
```

切换时：
- `useSessionLifecycle` 里 fire 一个 `set_session_mode` mutation → `desktop-server` 把新的 tool manifest 发给 runtime
- `InputBar` 的 @mention 建议列表在 Wiki mode 下换成 `@concept/...` `@people/...` `@topic/...`
- PermissionDialog 的风险语言换一套（`write_page` 显示 "这个操作会修改 wiki/concept/llm-wiki.md" 而不是 "this tool will run bash"）
- `StatusLine` 右侧多一个 chip `wiki · codex`

### D10 · MinApp "WeChat Inbox"

`features/apps/minapps/WeChatInbox/`：

- 路由：`/apps/wechat-inbox`
- 导航注册：`config/minapps.ts` 加一条 `{ id: 'wechat-inbox', name: 'WeChat Inbox', icon: ..., keepAlive: true, isInternal: true }`
- 视觉：沿用 MinApp 详情页的 header + tool bar + content pool；内容区是**时间线 + 卡片**
- 功能：展示最近 50 条微信事件、每条的处理状态（received / downloading / extracting / ingested / maintained / failed）、点"打开"跳 `/wiki/wechat` 看 pipeline 细节、点"重试"触发 Rust 重跑

---

## 4. 信息架构

```
ClawWiki (Tauri, CCD chrome 零改动)
│
├── 首页 /home                   保留 CCD workbench
│   └── [sidebar]
│       ├── Wiki                 NEW 分组（深链到 /wiki/*）
│       │   ├── Dashboard
│       │   ├── Raw Library
│       │   ├── Pages
│       │   ├── Inbox (未读 badge)
│       │   └── WeChat Bridge
│       ├── Sessions             保留
│       ├── Search               保留
│       ├── Scheduled            保留
│       ├── Dispatch             保留
│       ├── Customize            保留
│       └── Settings             保留
│
├── 应用 /apps                    保留 MinApps 画廊
│   └── /apps/wechat-inbox       NEW 内置 MinApp
│
├── Wiki /wiki                   NEW 一级 Tab，内容区用暖色
│   ├── /wiki                    Wiki Dashboard
│   ├── /wiki/raw                Raw Library
│   ├── /wiki/pages              Page Explorer (Concepts/People/Topics/Compare/Changelog)
│   ├── /wiki/pages/:slug        Page Detail（Lora 衬线 + backlinks aside）
│   ├── /wiki/graph              Knowledge Graph
│   ├── /wiki/schema             Schema Editor
│   ├── /wiki/inbox              Maintenance Inbox
│   └── /wiki/wechat             WeChat Bridge
│
├── Code /code                   保留（cliTool 加 "Claude Code Desktop"）
└── 设置 /settings
    ├── Account                  保留
    ├── Billing                  保留（cloud-accounts-sync 改走 Rust）
    ├── Token Broker             NEW
    ├── WeChat Bridge            NEW
    ├── Wiki Storage             NEW
    ├── Providers / MCP / Permissions / Data / About   保留
```

---

## 5. 代码改造清单

### 5.1 前端 `apps/desktop-shell`

| 动作 | 文件 | 说明 |
|---|---|---|
| **零改动** | `shell/{AppShell,TabBar,TabItem}.tsx` | 仅加一个 `WikiTabItem` + 8 条 `/wiki/*` 路由 |
| **零改动** | `features/session-workbench/*` | 加 `mode` prop |
| **修改** | `features/session-workbench/ContentHeader.tsx` | 加 mode 下拉 + Wiki mode 徽标 |
| **修改** | `features/session-workbench/StatusLine.tsx` | 显示 `mode: wiki` + `provider: codex/gpt-5.4` |
| **零改动** | `features/workbench/HomePage.tsx` | 在 sidebar PRIMARY_ITEMS 后加一条 Wiki 分组链接 |
| **零改动** | `features/apps/*` | 追加 `features/apps/minapps/WeChatInbox/` |
| **零改动** | `features/code-tools/*` | `cliTool` 列表加 "Claude Code Desktop" 选项 |
| **修改** | `features/billing/cloud-accounts-sync.ts` | 改走 Rust endpoint，删除前端明文 JSON 路径 |
| **修改** | `features/settings/SettingsPage.tsx` | `MENU_ITEMS` 加 `token-broker` / `wechat-bridge` / `wiki-storage` |
| **新增** | `features/wiki/*` 8 个页面 | 对应 8 条子路由 |
| **新增** | `features/wiki/ingest/*` | pipeline + 11 个 adapter + templates + persist |
| **新增** | `features/wiki/api/*` | client.ts + query.ts |
| **新增** | `state/wiki-store.ts` | Wiki 选中页、Inbox 未读、Maintenance 进行中 |
| **新增 deps** | `@clawwiki/defuddle-fork` · `obsidian-clipper` (用 `/api`) · `pdfjs-dist` · `ulid` | |

### 5.2 Rust `rust/crates/*`

| 动作 | crate | 说明 |
|---|---|---|
| **extend** | `desktop-core::managed_auth` | `CloudManaged` source + import/list/clear |
| **extend** | `desktop-server` | `/api/desktop/cloud/codex-accounts/*` |
| **extend** | `desktop-server` | `/v1/{chat/completions,messages,models}` Broker 代理 |
| **extend** | `desktop-server` | `/api/broker/{status,launch-client}` |
| **new** | `wiki-store` | `~/.clawwiki/` 文件系统后端 + manifest + frontmatter 校验 |
| **new** | `wiki-maintainer` | 触发型 agent loop |
| **new** | `wiki-ingest` | `POST /api/wiki/raw/ingest` + `/api/wiki/ingest/{voice,image,pptx,docx,video}` |
| **new** | `wechat-bridge` | WebSocket 订阅 `trade-service /ws/wechat-inbox` → Tauri event |

### 5.3 云端新服务

| 服务 | 端口 | 路由 |
|---|---|---|
| `wechat-ingest` | 8904 | `POST /api/v1/wechat/webhook` · `GET /api/v1/wechat/inbox?user_id=` · `GET /api/v1/wechat/blob/:id` · WebSocket `/ws/wechat-inbox?token=` |

---

## 6. MVP 路线（8 周）

| Sprint | 周 | 交付 | 成功标准 |
|---|---|---|---|
| **S1** | W1-2 | `wiki-store` + `~/.clawwiki` 布局 + `CLAUDE.md`/`AGENTS.md` 初稿 + `/wiki/raw` `/wiki/pages` 空壳 + 手动 `POST /api/wiki/raw/ingest` + TabBar 新增 Wiki Tab | 手工丢 md 进 raw/ 并在 UI 列表看到 |
| **S2** | W3 | fork defuddle + `wechat.ts` extractor + `obsidian-clipper/api` 前端封装 + URL ingestion 按钮 | 粘贴 mp.weixin URL，10s 内 raw/ 多一份格式良好 md |
| **S3** | W4 | `wiki-maintainer` MVP（engram 风格）+ SessionWorkbench mode 下拉 + PermissionDialog 工具集 | ingest 一个 raw 后自动生成 1-3 个 wiki/page + log.md |
| **S4** | W5 | `wechat-ingest` 云服务（文本 + URL） + WebSocket → 桌面 tray + `/wiki/wechat` 页面 + WeChat Inbox MinApp | 微信发 mp 链接到 bot，3s 内桌面 Inbox 看到卡片 |
| **S5** | W6 | Token Broker（`CloudManaged` source + `/v1/*` 代理 + Launch CCD 按钮） | 外部 CCD 通过本机 Broker 用到订阅 Codex |
| **S6** | W7 | 语音/图片/PDF/PPT adapter + 对应 Rust 端点 + Inbox 页面 + Graph 空实现 | 微信发语音和 PPT，raw/ 出 md |
| **S7** | W8 | Schema Editor + 陈旧度 lint + 冲突检测 + log.md/index.md 自动重建 | `/wiki/inbox` 有真实告警可触发修复 |
| **Backlog** | - | sage-wiki 5-pass compiler · prompt cache · Batch API · FTS5 · 向量 · 完整 Graph · MCP server · 个人微信桥接 opt-in · Obsidian Vault overlay | |

---

## 7. 风险 / 留白

1. **微信合规**：企业微信最稳但需企业资质；个人微信桥封号风险真实。**双轨**：默认企微；个人微信桥作为 opt-in 加红字。
2. **mp.weixin DOM 漂移**：每周 fixture 回归（爬 5 篇不同号），防止提取器悄悄坏。
3. **Codex Batch API**：要确认 Codex 是否支持 Batch + 折扣；不支持则只用 prompt cache。
4. **Raw 持续膨胀**：抄 engram 的 `compress`；raw/ 原文永不删，zstd 压。
5. **隐私**：云侧只中继不入库、30 天 TTL、AES-GCM 加密 blob（key 由 user-service 派发）。**原文永不经任何第三方 LLM**——只经过用户自己订阅账号的 Codex。
6. **v1/middle-path 设计不要浪费**：`cloud-managed-integration.md` 的 Rust 改动、`warwolf-cc-switch-*` 的 Broker 设计 S5 一起吞下。
7. **Wiki mode 的 wiki tools 实现复杂度**：每个 tool 都需要 Rust 实现 + 前端 PermissionDialog 适配，S3 可能超期，需要从最小 6 个开始：`read_page`/`write_page`/`link_pages`/`touch_changelog`/`rebuild_index`/`mark_conflict`。

---

## 8. 视觉 Token（壳 vs 内容）

| 作用域 | 背景 | 前景 | Primary | 字体正文 | 字体阅读 | 圆角 |
|---|---|---|---|---|---|---|
| 壳（/home /apps /code /settings） | `#0b0b0f`（现状 CCD 灰黑）| `#e5e5ea` | `#8b5cf6` 蓝紫 | Inter / Geist Sans | - | 8 |
| 内容（/wiki/*） | `#FAF9F6` DeepTutor | `#2D2B28` | `#C35A2C` 烧陶橙 | Plus Jakarta Sans | **Lora 衬线**（正文） | 16 |

CSS 实现：`body[data-route-scope="wiki"] { --background: #FAF9F6; --primary: #C35A2C; ... }`，监听 `useLocation()` 设置 `data-route-scope` 属性。

---

## 9. 开放问题（等你补第 3 条要求后确认）

1. **Obsidian 协同**：是否要 `vault overlay` 模式（直接把 `~/.clawwiki/wiki/` 软链到 Obsidian vault）？middle-path §12 建议过，我暂时放 Backlog。
2. **团队多用户**：是否要多 `~/.clawwiki/` 工作区 + 团队共享 + 冲突合并？
3. **移动端**：是否要一个只看 Dashboard + Ask + WeChat Inbox 的轻量 app？
4. **导出**：`engram export` 类似的 markdown/JSON 导出给第三方 RAG？
5. **付费分层**：是否要按"同步账号数"/"ingest 次数"/"wiki 大小"分级？middle-path 没提。
6. **MCP 对外**：是否要像 sage-wiki 那样把 wiki store 做成 MCP server，让其它 agent（CCD/Cursor）直接读写我们的 wiki？

---

## 10. 线框图索引

HTML 文件：[`./wireframes-v2.html`](./wireframes-v2.html) · 10 张屏，每张都带 **Claude Code Desktop 风格的双行 TabBar（含 `Wiki` 一级 Tab + Row 2 session tabs）**，壳用冷色、内容用暖色。

| # | 屏 | 亮点 |
|---|---|---|
| 01 | Wiki · Dashboard | 双行 TabBar + HomePage sidebar 的 Wiki 分组；主区是 Wiki 复利指标 + 今日维护活动 + Inbox 预览 + Ask 快捷输入 |
| 02 | Wiki · Raw Library | 来源过滤（含 wechat 标签）+ 拖拽 + 源列表 |
| 03 | Wiki · Page Explorer | Concepts/People/Topics/Compare/Changelog tabs + 卡片网格 |
| 04 | Wiki · Page Detail | 左 Lora 衬线正文 · 右 backlinks/sources/maintenance aside |
| 05 | Ask Session · Wiki Mode | ContentHeader 的 mode 下拉展开为 Wiki，使用 wiki tool 集，PermissionDialog 拦截 `write_page` |
| 06 | Wiki · WeChat Bridge | Bot 绑定卡 + Inbox tray + Pipeline 状态灯（defuddle → clipper → maintainer 三档） |
| 07 | /apps/wechat-inbox MinApp | 微信事件时间线；一条消息从 webhook 到 raw 到 maintainer 的全链路可视化 |
| 08 | Wiki · Graph | 节点=页面 · 颜色=fresh/stale/conflict · 右上 legend |
| 09 | Wiki · Inbox + Schema 分屏 | 左 Inbox triage · 右 Schema Editor（Monaco dark），演示 AI 提的 schema change proposal |
| 10 | Settings · Token Broker + WeChat | 账号池（cloud-managed 只读 + local）· Broker 状态 · Launch CCD 按钮 · WeChat webhook 配置 |
