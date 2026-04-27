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

This document records known issues from the Phase 5 + Phase 6 demo-stabilization pass.
It is not a feature spec. It is the operational truth for demo readiness and follow-up work.

## Phase 5 + 6 Validation Summary

Date: 2026-04-27

Scope:

- OpenAI-compatible multi-turn tool calling through DeepSeek Chat.
- Safe-only tool exposure policy for OpenAI-compatible providers.
- Ask UI interrupt handling, elapsed timing, tool cards, and persisted history restore.
- Settings disclosure for allowed and blocked tool capabilities.

Overall status:

- Backend tool-calling path is functional.
- Safe-only tool whitelist is effective.
- Pure writing and small-talk prompts stay text-only.
- Write and shell requests are refused directly instead of probing with read/search tools.
- Ask UI interrupt no longer crashes the page in the tested path.
- Header elapsed timing now shows real completed duration and updates while active.

## Resolved In Phase 5 + 6

### Esc interrupt error boundary crash

Previous behavior:

- Pressing `Esc` while a long response was streaming could produce the error boundary:
  `Cannot read properties of undefined (reading 'messages')`.

Current behavior:

- Interrupting a streaming response keeps the page mounted.
- The partial response remains visible with the stopped marker.
- The composer becomes usable again and a new message can be sent.

### Header elapsed time stuck at `0.0s`

Previous behavior:

- Long and multi-turn requests showed realistic token counts, but elapsed time could stay at `0.0s`.

Current behavior:

- Active turns update the elapsed time on a one-second interval.
- Completed turns keep the final measured duration.

### Unsafe tool requests were not direct enough

Previous behavior:

- Requests such as editing `README.md` could lead the model to read/search first, then explain that writing was unavailable.

Current behavior:

- File write, shell execution, and agent orchestration requests are treated as policy-refusal prompts.
- The model answers directly with the policy limitation and safe alternatives.

## Remaining Product And UX Limitations

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

### Tool capability management is read-only in Settings

Current behavior:

- Settings -> Advanced shows which tools are allowed or blocked.
- The UI does not yet provide toggles for changing `ToolExposurePolicy`.

Follow-up:

- Add Settings UI for user-level `ToolExposurePolicy`.
- Add explicit confirmation before enabling write or shell tools.

### MCP integration is not enabled on the OpenAI-compatible path

MCP tools may be visible in developer settings, but they are not part of the default OpenAI-compatible tool exposure policy.

### Final synthesis quality still depends on provider behavior

The final synthesis turn now disables tools with `tool_choice=none` and injects a final-answer instruction.
This materially improves stability, but provider output quality still depends on model behavior and source quality.

### Blocking tool cancellation cannot kill a running blocking thread

Current behavior:

- UI returns to an interrupted state quickly.
- The underlying blocking tool task may continue in the background until the tool returns.

Follow-up:

- Add cancellation support inside individual tools where practical.

## Build And Console Warnings

These warnings are known and do not currently block local use:

- CSS parsing warnings from Tailwind arbitrary variants such as `ask:bind` and `composer:handleSend`.
- Dynamic import chunk warning around desktop bootstrap code.
- Bundle size warning for chunks larger than 500 KB.
- Rust future-incompatibility warning from `redis v0.25.4`.
- Existing Rust test warnings in `wiki_ingest` and `desktop-core` test modules.

Follow-up:

- Replace unsupported arbitrary CSS variants or isolate them.
- Review manual chunking strategy for desktop bootstrap and large feature modules.
- Track dependency upgrades for future-incompat warnings.

## Backlog

- Add user-editable `ToolExposurePolicy` controls in Settings.
- Add automated end-to-end smoke for `user -> tool_use -> tool_result -> final answer`.
- Add structured telemetry for prompt intent decisions and tool policy filtering.
- Improve final synthesis prompting with provider-specific examples.
- Add stronger prompt guidance to prevent DSML/control-token leakage at the source.
- Add true cancellation for long-running network tools.

## Phase 5 + 6 Scenario Results

| Scenario | Expected | Result |
| --- | --- | --- |
| Ordinary text conversation | Text reply plus real elapsed | Normal. Text reply rendered without tools; elapsed showed a non-zero final duration. |
| Pure writing | Direct writing, no tools | Normal. Writing prompt used `tool_choice=none` and produced text directly. |
| Search request | WebSearch/WebFetch plus summary | Normal. Safe tools were exposed and completed; final answer was produced. |
| Blocked write tool | Direct policy refusal | Normal. No write executed, no permission prompt, and the answer explained the safety policy. |
| Blocked shell execution | Direct policy refusal | Normal. No shell tool was exposed; answer suggested manual execution and pasting errors back. |
| Multi-turn search/read/summarize | Tool loop plus final synthesis | Normal in smoke. Tools completed and final synthesis turn used `tool_choice=none`. |
| User interrupt | Immediate stop, no crash | Normal in smoke. Partial content remained and a new message worked afterwards. |
| Long content generation | Complete streaming | Normal in smoke; elapsed timer updated and then settled. |
| Tool failure | Error result shown | Normal from Step 4.8 validation. Failed `WebFetch` rendered as a failed tool result and assistant explained the failure. |
| Model switching | Accurate capability state | Limited validation. DeepSeek Chat capability display is correct; other providers were not configured locally. |
| History restore | Messages preserved | Normal. Reload restored conversation state. |
| DSML token check | No visible control token | Normal in smoke. No visible DSML/control token leak observed. |
| Settings tool capability section | User-readable policy disclosure | Normal. Advanced settings show allowed and blocked tool categories. |
| Active elapsed timer | 1s interval updates | Normal in smoke; completed turns retain final duration. |
