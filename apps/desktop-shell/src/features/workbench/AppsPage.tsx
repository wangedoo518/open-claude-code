import { Code } from "lucide-react";
import { useTranslation } from "react-i18next";
import { useNavigate } from "react-router-dom";
import { CherryOpenClawIcon } from "@/components/icons/CherryIcons";
import { buildHomeSectionHref } from "./tab-helpers";

const APPS = [
  {
    id: "code",
    label: "Code",
    icon: Code,
    gradient: "linear-gradient(135deg, #1F2937, #374151)",
  },
  {
    id: "openclaw",
    label: "OpenClaw",
    icon: CherryOpenClawIcon,
    gradient: "linear-gradient(135deg, #EF4444, #B91C1C)",
  },
] as const;

export function AppsPage() {
  const { t } = useTranslation();
  const navigate = useNavigate();

  return (
    <div className="h-full overflow-auto bg-background">
      <div className="mx-auto flex w-full max-w-[720px] flex-col gap-5 py-[50px]">
        <section className="flex flex-col gap-2">
          <h1 className="m-0 px-[36px] text-subhead font-semibold text-foreground opacity-80">
            {t("page.apps")}
          </h1>

          <div className="grid grid-cols-6 gap-2 px-2">
            {APPS.map((app) => {
              const Icon = app.icon;

              return (
                <button
                  key={app.id}
                  className="group flex appearance-none flex-col items-center gap-1 rounded-2xl border-0 bg-transparent px-1 py-2 text-center transition-transform duration-200 ease-out hover:scale-[1.05] active:scale-[0.95]"
                  onClick={() => {
                    if (app.id === "code") {
                      navigate("/code");
                      return;
                    }

                    navigate(buildHomeSectionHref("openclaw"));
                  }}
                >
                  <div className="relative flex h-14 w-14 items-center justify-center">
                    <div
                      className="flex h-14 w-14 items-center justify-center rounded-2xl text-white shadow-[0_2px_4px_rgba(0,0,0,0.1)]"
                      style={{ background: app.gradient }}
                    >
                      <Icon className="h-7 w-7 text-white" />
                    </div>
                  </div>
                  <div className="w-full overflow-hidden text-ellipsis whitespace-nowrap pt-0 text-center text-body-sm text-foreground">
                    {app.label}
                  </div>
                </button>
              );
            })}
          </div>
        </section>
      </div>
    </div>
  );
}
