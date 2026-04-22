/**
 * AskMarkdown — shared assistant-body Markdown renderer (A5).
 *
 * Before A5, `Message.tsx::MarkdownContent` rendered the final
 * assistant body with full Markdown support (heading / list /
 * blockquote / table / link / fenced code with Prism) while
 * `StreamingMessage` used a stripped-down ReactMarkdown that only
 * handled `p` / `code` / `pre` — so the visual bounce when a turn
 * completed was large (lists collapsed to plain paragraphs,
 * headings lost their hierarchy, code blocks lost their frame).
 *
 * A5 puts streaming and final on the same renderer. A single prop
 * (`streaming`) switches the code block between:
 *   - streaming=true  → muted "still-writing" shell, no Prism
 *   - streaming=false → full Prism-highlighted oneDark block
 *
 * Everything else (paragraphs, lists, headings, blockquotes, tables,
 * links) stays identical across both states. That means when a turn
 * flips from streaming to complete, the only visible change is the
 * code blocks picking up syntax highlighting — no layout shift.
 */

import { memo } from "react";
import ReactMarkdown from "react-markdown";
import { AskCodeBlock } from "./AskCodeBlock";

export interface AskMarkdownProps {
  content: string;
  /**
   * When true, fenced code blocks render in their streaming variant
   * (muted header, no Prism); inline code and all other elements
   * are identical.
   */
  streaming?: boolean;
}

/**
 * Heuristic: a streaming chunk may end mid-fence ("\n```python\n…")
 * and ReactMarkdown's default behaviour is to treat the trailing
 * unterminated fence as literal text, collapsing the would-be code
 * block into a single paragraph. That looks jarring — the UI shows
 * plain text while the user can see the model is clearly writing
 * code. To smooth this over we detect a trailing open fence and
 * append a synthetic closing fence before handing the string to
 * ReactMarkdown. The extra fence is visually a no-op because the
 * block renders in its streaming variant, but it gives us the
 * container and frame the user expects to see.
 */
function stabilizeOpenFence(content: string): string {
  // Count standalone ``` fences at start-of-line. Odd = one is open.
  const fences = content.match(/^```[^\n`]*$/gm);
  if (!fences || fences.length % 2 === 0) return content;
  // Append a synthetic closing fence. Keep it on its own line so
  // ReactMarkdown parses it cleanly.
  return content + (content.endsWith("\n") ? "```" : "\n```");
}

export const AskMarkdown = memo(function AskMarkdown({
  content,
  streaming = false,
}: AskMarkdownProps) {
  const normalized = streaming ? stabilizeOpenFence(content) : content;

  return (
    <ReactMarkdown
      components={{
        code({ className, children, ...props }) {
          const match = /language-(\w+)/.exec(className || "");
          const codeString = String(children).replace(/\n$/, "");

          if (match) {
            return (
              <AskCodeBlock
                language={match[1]}
                code={codeString}
                streaming={streaming}
              />
            );
          }

          return (
            <code
              className="rounded bg-muted/60 px-1.5 py-0.5 font-mono text-[13px] text-foreground"
              {...props}
            >
              {children}
            </code>
          );
        },
        pre({ children }) {
          return <>{children}</>;
        },
        p({ children }) {
          return <p className="mb-2 last:mb-0">{children}</p>;
        },
        ul({ children }) {
          return <ul className="mb-2 list-disc pl-5 last:mb-0">{children}</ul>;
        },
        ol({ children }) {
          return <ol className="mb-2 list-decimal pl-5 last:mb-0">{children}</ol>;
        },
        li({ children }) {
          return <li className="mb-0.5">{children}</li>;
        },
        h1({ children }) {
          return (
            <h1 className="mb-2 mt-3 text-base font-bold first:mt-0">
              {children}
            </h1>
          );
        },
        h2({ children }) {
          return (
            <h2 className="mb-2 mt-3 text-head font-bold first:mt-0">
              {children}
            </h2>
          );
        },
        h3({ children }) {
          return (
            <h3 className="mb-1.5 mt-2.5 text-sm font-semibold first:mt-0">
              {children}
            </h3>
          );
        },
        blockquote({ children }) {
          return (
            <blockquote className="mb-2 border-l-[3px] border-muted-foreground/30 pl-3 italic text-muted-foreground last:mb-0">
              {children}
            </blockquote>
          );
        },
        table({ children }) {
          return (
            <div className="mb-2 overflow-x-auto last:mb-0">
              <table className="w-full border-collapse text-body-sm">
                {children}
              </table>
            </div>
          );
        },
        th({ children }) {
          return (
            <th className="border border-border/50 bg-muted/50 px-2.5 py-1.5 text-left font-semibold">
              {children}
            </th>
          );
        },
        td({ children }) {
          return (
            <td className="border border-border/50 px-2.5 py-1.5">{children}</td>
          );
        },
        hr() {
          return <hr className="my-3 border-border/50" />;
        },
        a({ href, children }) {
          return (
            <a
              href={href}
              className="text-[color:var(--color-label-you,rgb(37,99,235))] underline decoration-[color:var(--color-label-you,rgb(37,99,235))]/30 hover:decoration-[color:var(--color-label-you,rgb(37,99,235))]"
              target="_blank"
              rel="noopener noreferrer"
            >
              {children}
            </a>
          );
        },
      }}
    >
      {normalized}
    </ReactMarkdown>
  );
});
