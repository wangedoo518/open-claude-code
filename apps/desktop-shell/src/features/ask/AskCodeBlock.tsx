/**
 * AskCodeBlock — Claude-style code block used by both completed and
 * streaming assistant messages.
 *
 * Goals:
 *  - Visual parity between streaming and final states. When streaming,
 *    a fenced block with an open fence renders as a muted "正在编写"
 *    shell with the partial code inside; once the fence closes (or the
 *    turn finishes), the same container smoothly promotes to the full
 *    Prism-highlighted style.
 *  - Language label + copy button match the Claude chat surface
 *    (rounded corners, quiet text, hover-revealed actions).
 *  - Copy action yields a short "已复制" acknowledgement; no toast
 *    spam, no modal.
 *
 * The component is imported from both `Message.tsx::AssistantMessage`
 * and `StreamingMessage`, so any styling change lands in both places
 * simultaneously.
 */

import { memo, useState } from "react";
import { Prism as SyntaxHighlighter } from "react-syntax-highlighter";
import { oneDark } from "react-syntax-highlighter/dist/esm/styles/prism";
import { Check, Copy } from "lucide-react";

export interface AskCodeBlockProps {
  language: string;
  code: string;
  /**
   * When true, the block renders a subdued "still-streaming" state —
   * the language header shows a tiny shimmer dot and the code body
   * uses the same palette as the final state but omits syntax
   * highlighting to avoid flickering partial tokens.
   */
  streaming?: boolean;
}

export const AskCodeBlock = memo(function AskCodeBlock({
  language,
  code,
  streaming = false,
}: AskCodeBlockProps) {
  const [copied, setCopied] = useState(false);

  const handleCopy = () => {
    void navigator.clipboard.writeText(code);
    setCopied(true);
    setTimeout(() => setCopied(false), 1800);
  };

  return (
    <div className="group/code my-3 overflow-hidden rounded-lg border border-border/50 bg-[#1f2937]">
      {/* Language header bar */}
      <div className="flex items-center justify-between border-b border-white/10 px-3 py-2">
        <div className="flex items-center gap-2">
          {streaming && (
            <span
              className="inline-block size-1.5 animate-pulse rounded-full bg-[#9ca3af]"
              aria-hidden
            />
          )}
          <span className="text-[11px] font-medium uppercase tracking-wider text-[#9ca3af]">
            {language || (streaming ? "code" : "text")}
          </span>
        </div>
        <button
          type="button"
          onClick={handleCopy}
          className="flex items-center gap-1 rounded px-1.5 py-0.5 text-[11px] text-[#9ca3af] opacity-0 transition-opacity hover:text-white focus-visible:opacity-100 group-hover/code:opacity-100"
        >
          {copied ? (
            <>
              <Check className="size-3" /> 已复制
            </>
          ) : (
            <>
              <Copy className="size-3" /> 复制
            </>
          )}
        </button>
      </div>

      {streaming ? (
        // While streaming, avoid running Prism on partial tokens — it
        // flickers whenever a half-written identifier / string crosses
        // the lexer boundary. Use the same palette so the visual jump
        // on completion is minimal.
        <pre
          className="overflow-x-auto p-4 font-mono text-[13px] leading-[1.7] text-[#e5e7eb]"
          style={{
            background: "#1f2937",
            fontFamily:
              "var(--font-family-dt-mono, 'JetBrains Mono', 'Cascadia Code', 'Fira Code', monospace)",
          }}
        >
          {code}
        </pre>
      ) : (
        <SyntaxHighlighter
          language={language || "text"}
          style={oneDark}
          customStyle={{
            margin: 0,
            padding: "1rem",
            fontSize: "13px",
            lineHeight: "1.7",
            background: "#1f2937",
            borderRadius: 0,
          }}
          codeTagProps={{
            style: {
              fontFamily:
                "var(--font-family-dt-mono, 'JetBrains Mono', 'Cascadia Code', 'Fira Code', monospace)",
            },
          }}
        >
          {code}
        </SyntaxHighlighter>
      )}
    </div>
  );
});
