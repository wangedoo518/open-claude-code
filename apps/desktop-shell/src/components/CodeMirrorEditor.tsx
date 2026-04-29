import CodeMirror from "@uiw/react-codemirror";
import { markdown } from "@codemirror/lang-markdown";
import { yaml } from "@codemirror/lang-yaml";
import { useMemo } from "react";

type CodeMirrorLanguage = "markdown" | "yaml";

interface CodeMirrorEditorProps {
  value: string;
  onChange: (value: string) => void;
  language?: CodeMirrorLanguage;
  minHeight?: string;
  readOnly?: boolean;
  ariaLabel?: string;
}

export function CodeMirrorEditor({
  value,
  onChange,
  language = "markdown",
  minHeight = "400px",
  readOnly = false,
  ariaLabel,
}: CodeMirrorEditorProps) {
  const extensions = useMemo(
    () => [language === "yaml" ? yaml() : markdown()],
    [language],
  );

  return (
    <div
      className="buddy-codemirror rounded-md border border-border bg-background"
      style={{ minHeight }}
    >
      <CodeMirror
        value={value}
        onChange={onChange}
        extensions={extensions}
        basicSetup={{
          foldGutter: true,
          highlightActiveLine: true,
          highlightActiveLineGutter: true,
          lineNumbers: true,
        }}
        editable={!readOnly}
        readOnly={readOnly}
        height={minHeight}
        aria-label={ariaLabel}
      />
    </div>
  );
}
