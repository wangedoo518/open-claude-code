/**
 * SettingsModal — global settings dialog (08-settings-modal.md).
 *
 * Replaces the /settings route with a floating modal accessible from anywhere.
 * Left tab menu + right content area.
 */

import * as Dialog from "@radix-ui/react-dialog";
import { X } from "lucide-react";
import { useSettingsStore } from "@/state/settings-store";
import { SettingsPage } from "./SettingsPage";

export function SettingsModal() {
  const open = useSettingsStore((s) => s.settingsModalOpen);
  const close = useSettingsStore((s) => s.closeSettingsModal);

  return (
    <Dialog.Root open={open} onOpenChange={(o) => { if (!o) close(); }}>
      <Dialog.Portal>
        <Dialog.Overlay className="fixed inset-0 z-40 bg-black/40 data-[state=open]:animate-fade-in" />
        <Dialog.Content
          className="fixed left-1/2 top-1/2 z-50 flex max-h-[85vh] w-full max-w-3xl -translate-x-1/2 -translate-y-1/2 flex-col overflow-hidden rounded-3xl border border-[var(--color-border)] bg-[var(--color-background)] shadow-lg data-[state=open]:animate-fade-in"
        >
          {/* Header */}
          <div className="flex h-12 shrink-0 items-center justify-between border-b border-[var(--color-border)] px-4">
            <Dialog.Title className="text-[14px] font-semibold text-[var(--color-foreground)]">
              Settings
            </Dialog.Title>
            <Dialog.Close className="rounded-md p-1 text-[var(--color-muted-foreground)] hover:bg-[var(--color-accent)] hover:text-[var(--color-foreground)]">
              <X className="size-4" />
            </Dialog.Close>
          </div>

          {/* Body — reuse existing SettingsPage content */}
          <div className="flex-1 overflow-y-auto">
            <SettingsPage />
          </div>
        </Dialog.Content>
      </Dialog.Portal>
    </Dialog.Root>
  );
}
