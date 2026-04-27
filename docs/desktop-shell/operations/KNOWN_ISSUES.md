---
title: Desktop Shell Known Issues
doc_type: operation
status: active
owner: desktop-shell
last_verified: 2026-04-27
source_of_truth: true
related:
  - docs/desktop-shell/operations/README.md
  - docs/desktop-shell/architecture/overview.md
---

# Desktop Shell Known Issues

This document records known issues from the Step 4 Ask tool-calling validation pass.
It is not a feature spec. It is the operational truth for demo readiness and follow-up work.

## Step 4 Validation Summary

Date: 2026-04-27

Scope:

- OpenAI-compatible tool calling through DeepSeek Chat.
- Safe-only tool exposure policy.
- Multi-turn loop: user -> assistant tool_use -> tool_result -> assistant final answer.
- Ask UI status visibility, tool cards, and persisted history restore.

Overall status:

- Backend tool-calling path is functional.
- Safe-only tool whitelist is effective.
- Product honesty is mostly correct for available tools.
- Two UI/status issues remain high priority before external demo.

## Blocking Before External Demo

### Esc interrupt can briefly crash the Ask UI

Observed behavior:

- Pressing `Esc` while a long response is streaming produced the error boundary:
  `Cannot read properties of undefined (reading 'messages')`.
- After reload, the interrupted conversation was restored with a clean
  `cancelled by user` message.

Expected behavior:

- Interrupt should stop the active turn without crashing the page.
- Existing streamed content should remain visible.
- The user should see a clear interrupted state and recovery actions.

Priority: P0 for public demo, P1 for internal use.

### Header elapsed time is stuck at `0.0s`

Observed behavior:

- Long and multi-turn requests showed realistic token counts, but elapsed time stayed at `0.0s`.
- This happened for ordinary text, tool calls, multi-turn search, and long generation.

Expected behavior:

- Active turns should show a live timer.
- Completed turns should show elapsed time from the last user message to the final assistant or tool message.

Priority: P1.

### Final synthesis after tools is not reliable enough

Observed behavior:

- Tool execution succeeds, but some search tasks stop after additional reading/extraction language instead of producing the requested concise final summary.
- This was visible in weather and DeepSeek V4 summary prompts.

Expected behavior:

- Once tool results are available, the model should produce the requested final answer.
- If a tool result is insufficient, the model should state that clearly instead of continuing to plan extraction.

Priority: P1 before external demo.

## Product And UX Limitations

### DeepSeek Reasoner does not support tools

DeepSeek Reasoner remains text/reasoning only. Tool dots and capability hints should continue to reflect this.

### Safe-only tool policy is enabled for OpenAI-compatible providers

Only the default safe read-only tools are exposed:

- `WebSearch`
- `WebFetch`
- `read_file`
- `glob_search`
- `grep_search`

Filesystem writes, shell execution, agent orchestration, and MCP-style tools are not exposed by default.

### File write and shell tools require future opt-in UI

Current behavior:

- `write_file`, `edit_file`, `bash`, `PowerShell`, and similar tools are filtered out.
- The model should not execute write or shell actions on the OpenAI-compatible path.

Follow-up:

- Add Settings UI for user-level `ToolExposurePolicy`.
- Add clear safety copy before enabling write or shell tools.

### MCP integration is not enabled on the OpenAI-compatible path

MCP tools may be visible in developer settings, but they are not part of the default OpenAI-compatible tool exposure policy.

### Model may overuse tools for pure writing tasks

Observed behavior:

- A pure long-form writing request triggered web/search tools before writing.

Expected behavior:

- Pure composition should normally stay text-only unless the user asks for search, verification, or current information.

Follow-up:

- Tighten system prompt and tool-use instructions.
- Consider an intent gate before exposing tools for a turn.

### Write requests are safe but not direct enough

Observed behavior:

- A request to modify `README.md` did not write files and did not trigger permission prompts.
- The model still read/searched files instead of immediately explaining that write tools are disabled in the current policy.

Expected behavior:

- The model should explain the limitation and offer safe alternatives.

## Build And Console Warnings

These warnings are known and do not currently block local use:

- CSS parsing warnings from Tailwind arbitrary variants such as `ask:bind` and `composer:handleSend`.
- Dynamic import chunk warning around desktop bootstrap code.
- Bundle size warning for chunks larger than 500 KB.

Follow-up:

- Replace unsupported arbitrary CSS variants or isolate them.
- Review manual chunking strategy for desktop bootstrap and large feature modules.

## Backlog

- Fix `Esc` interrupt error boundary crash.
- Fix Ask header elapsed timer aggregation.
- Add final synthesis guard after tool results.
- Add tool-use intent routing to reduce unnecessary searches.
- Add Settings UI for `ToolExposurePolicy`.
- Add user-facing copy for disabled write and shell tools.
- Add stronger prompt guidance to prevent DSML/control-token leakage at the source.
- Add automated end-to-end smoke for `user -> tool_use -> tool_result -> final answer`.

## Step 4.8 Scenario Results

| Scenario | Expected | Result |
| --- | --- | --- |
| Ordinary text conversation | Text reply | Normal. Text reply rendered without tools. Elapsed displayed `0.0s`. |
| Single tool call | One tool plus summary | Partial. Tools executed, but final one-sentence summary quality was weak. |
| Multi-tool call | Multi-turn search/read/summarize | Partial. Multiple tools completed, but final requested two-paragraph synthesis was unreliable. |
| Blocked write tool | No unsafe write | Normal for safety. No write executed and no permission prompt appeared. Copy needs improvement. |
| User interrupt | Immediate stop | Abnormal. Page briefly hit an error boundary, then restored after reload. |
| Long content generation | Complete streaming | Normal. Long response rendered, but tool overuse and elapsed issue were observed. |
| Tool failure | Error result shown | Normal. Failed `WebFetch` rendered as a failed tool result and assistant explained the failure. |
| Model switching | Accurate capability state | Skipped for actual switching. Only DeepSeek Chat was configured. Capability display was correct. |
| History restore | Messages preserved | Normal. Reload restored conversation and cancelled state. |
| DSML token check | No visible control token | Normal. No visible DSML/control token leak observed. |

