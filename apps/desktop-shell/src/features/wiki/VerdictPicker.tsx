import { useEffect, useRef, useState } from "react";
import { Sparkle, ArrowDownToLine, HelpCircle } from "lucide-react";

/**
 * Slice E17 — Decision Retrospective UI.
 *
 * Sits in the WikiArticle metadata row. Lets the user attach a
 * post-hoc verdict (`should_continue` / `should_let_go` /
 * `inconclusive`) and an optional reason to the page they're reading.
 *
 * Buddy positioning: the verdict is what closes the curation loop —
 * "事后看，这条灵感值得继续投入还是可以放下". The picker is reversible
 * (just another POST), so users can change their mind without
 * destructive consequences.
 */

export type Verdict = "should_continue" | "should_let_go" | "inconclusive";

const CHOICES: ReadonlyArray<{
  id: Verdict;
  label: string;
  hint: string;
  Icon: typeof Sparkle;
}> = [
  {
    id: "should_continue",
    label: "继续投入",
    hint: "这条想法事后看仍值得继续追",
    Icon: Sparkle,
  },
  {
    id: "should_let_go",
    label: "可以放下",
    hint: "投入了一段时间，没有产出；可以冷却",
    Icon: ArrowDownToLine,
  },
  {
    id: "inconclusive",
    label: "暂未决定",
    hint: "证据不足以下判断；先留着观察",
    Icon: HelpCircle,
  },
];

export interface VerdictPickerProps {
  currentVerdict?: Verdict | null;
  reason?: string;
  onChange: (verdict: Verdict, reason: string) => void | Promise<void>;
}

export function VerdictPicker({
  currentVerdict,
  reason,
  onChange,
}: VerdictPickerProps) {
  const [open, setOpen] = useState(false);
  const [draftReason, setDraftReason] = useState(reason ?? "");
  const [saving, setSaving] = useState<Verdict | null>(null);

  // Review I2 — re-init the textarea from the latest server-side
  // reason every time the popover transitions closed→open, so the
  // user can't accidentally save a stale draft over a newer
  // persisted value (e.g. another tab edited the page, then this
  // tab refetched). Tracking the previous-open state via ref keeps
  // mid-edit typing intact while the popover is open — we only
  // reset on the open transition itself, not on every reason
  // refetch.
  const wasOpenRef = useRef(false);
  useEffect(() => {
    if (open && !wasOpenRef.current) {
      setDraftReason(reason ?? "");
    }
    wasOpenRef.current = open;
  }, [open, reason]);

  const current = CHOICES.find((c) => c.id === currentVerdict);

  return (
    <div className="ds-verdict-picker">
      <button
        type="button"
        className="ds-verdict-trigger"
        onClick={() => setOpen((v) => !v)}
        title="事后判断：这条灵感是否值得继续投入"
        data-active={open || undefined}
      >
        {current ? (
          <>
            <current.Icon className="size-3" aria-hidden />
            <span>{current.label}</span>
          </>
        ) : (
          <span className="text-muted-foreground">事后判断</span>
        )}
      </button>
      {open ? (
        <div className="ds-verdict-popover" role="dialog" aria-label="设置事后判断">
          <p className="ds-verdict-help">
            事后看，这条想法值得继续投入还是可以放下？
          </p>
          <div className="ds-verdict-options">
            {CHOICES.map((choice) => (
              <button
                key={choice.id}
                type="button"
                className="ds-verdict-option"
                data-active={currentVerdict === choice.id || undefined}
                disabled={saving !== null}
                onClick={async () => {
                  setSaving(choice.id);
                  try {
                    await onChange(choice.id, draftReason);
                    setOpen(false);
                  } finally {
                    setSaving(null);
                  }
                }}
              >
                <choice.Icon className="size-3.5 mt-0.5 shrink-0" aria-hidden />
                <div className="flex flex-col items-start gap-0.5">
                  <span className="font-medium">{choice.label}</span>
                  <span className="text-[10px] leading-tight text-muted-foreground">
                    {choice.hint}
                  </span>
                </div>
              </button>
            ))}
          </div>
          <textarea
            className="ds-verdict-reason"
            placeholder="为什么这么判断？(可选, ≤ 200 字)"
            value={draftReason}
            onChange={(e) => setDraftReason(e.target.value)}
            maxLength={200}
            rows={2}
          />
        </div>
      ) : null}
    </div>
  );
}
