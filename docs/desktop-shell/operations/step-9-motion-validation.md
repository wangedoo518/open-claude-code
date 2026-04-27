# Step 9 Motion Validation

Date: 2026-04-27

Scope: cross-model Ask motion primitives and streaming reveal behavior.

## Changes Validated

- Shared motion tokens and state-dot classes are defined in `apps/desktop-shell/src/globals.css`.
- Streaming content now uses `streamingBuffer` for backend-accurate accumulation and `streamingContent` for visible throttled reveal.
- `useStreamingReveal` is mounted once at `AskWorkbench` level and is provider-agnostic.
- Ask header, streaming status rows, composer, and tool rows use state-derived visual classes rather than provider-derived branches.

## Validation Matrix

| # | Scenario | Prompt | Result |
|---|---|---|---|
| 1 | Short text | `你好，只回复一句用于 Step 9.3 状态切换验证。` | Passed. Header reached completed state and answer rendered normally. |
| 2 | Long text | `写一段 500 字的春天散文，用中文。` | Passed. Completed in 6.6s with visible streaming content and no post-completion ghost text. |
| 3 | Single tool | `搜一下今天天气，用一句话总结。` | Passed. Tool group rendered as `访问网页 1 完成`, followed by assistant summary. |
| 4 | Multi-turn | `搜一下 deepseek v4 最新发布信息，读取相关页面并基于结果给我两段总结。` | Passed for UI behavior. Multiple tool groups completed and header metrics updated. Model still used process language in final content; this is backend prompt behavior, not motion behavior. |
| 5 | Interrupt | `请写一篇 3000 字的长文，主题是 AI 工具状态可见性。` then Esc | Passed. Header showed interrupted/stopped state, composer remained usable, and no React error boundary appeared. |
| 6 | Failure | `请用 WebFetch 读取 https://127.0.0.1:1/ 并总结结果；如果失败，请说明失败原因。` | Passed. Tool row showed failure state and assistant explained the connection failure. |
| 7 | Repeated chat turns | `你好` -> `怎么样` -> `再见` | Passed. All turns completed and composer stayed usable. |

## Browser Smoke Evidence

- Playwright CLI snapshot confirmed the Ask page renders the animated placeholder as visible overlay text while the native textarea placeholder stays empty.
- Playwright CLI snapshot confirmed completed tool groups render as a single tool group status line rather than leaving a running state.
- Console log contained existing development info/log entries only; no React error boundary or runtime error was observed during the Step 9.3 and Step 9.4 smoke tests.
- Screenshot artifact: `.playwright-cli/page-2026-04-27T13-12-37-081Z.png`.

## Performance Notes

The requested FPS/long-task check was attempted via Playwright CLI with a `requestAnimationFrame` sampler and `PerformanceObserver` for long tasks. The CLI-controlled browser session reported ~1s RAF intervals, which indicates the page was background-throttled or the automation session was not a reliable foreground performance environment. Therefore:

- Do not treat the automated RAF sample as a valid 60fps measurement.
- No user-visible stutter, layout flash, or delayed final flush was observed in the browser smoke tests.
- A reliable FPS number should be captured manually in Chrome DevTools Performance with the browser window foregrounded.

## Consistency Assessment

Current validation used DeepSeek plus different response shapes: plain text, long output, tool call, multi-turn, interrupted turn, failure turn, and repeated short turns. Because only DeepSeek is configured locally, cross-provider consistency was validated indirectly:

- Chunk pacing is normalized by `useStreamingReveal`, which reads only the provider-agnostic streaming buffer.
- State dots and composer motion read only conversation state/tone fields.
- No animation code branches on provider/model name.

## Follow-ups

- Capture a foreground Chrome DevTools Performance recording for a multi-turn run if exact FPS evidence is required.
- Re-run the same matrix when another provider is configured locally to confirm subjective parity across real provider streams.
