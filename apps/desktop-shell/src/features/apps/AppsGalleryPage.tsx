import { useMemo, useState } from "react";
import { useNavigate } from "react-router-dom";
import { Search } from "lucide-react";
import { Input } from "@/components/ui/input";
import { MinApp } from "@/components/MinApp/MinApp";
import { useMinapps } from "@/hooks/useMinapps";
import { ScrollArea } from "@/components/ui/scroll-area";
import WarwolfLogo from "@/assets/warwolf-logo.png";

/**
 * Apps gallery page rendered at `/apps`.
 *
 * Shows a grid of available apps matching cherry-studio's MinAppsPage layout:
 * - Centered grid with `grid-template-columns: repeat(auto-fill, 90px)`
 * - Search filter by name
 * - 25px gap between items
 * - Top header with search input
 */
export function AppsGalleryPage() {
  const navigate = useNavigate();
  const { minapps } = useMinapps();
  const [search, setSearch] = useState("");
  const launchers = useMemo(
    () => [
      {
        id: "code",
        name: "Code",
        url: "/code",
        logo: WarwolfLogo,
        description: "代码工具",
        kind: "route" as const,
      },
      ...minapps.map((app) => ({
        ...app,
        kind: "minapp" as const,
      })),
    ],
    [minapps]
  );

  const filteredApps = search
    ? launchers.filter(
        (app) =>
          app.name.toLowerCase().includes(search.toLowerCase()) ||
          app.url.toLowerCase().includes(search.toLowerCase())
      )
    : launchers;

  return (
    <div className="flex h-full flex-col overflow-hidden">
      {/* Header with search */}
      <div className="flex items-center justify-center gap-2.5 px-4 py-4">
        <div className="relative w-[30%] min-w-[200px]">
          <Input
            placeholder="Search apps..."
            className="h-8 rounded-full pl-9 text-sm"
            value={search}
            onChange={(e) => setSearch(e.target.value)}
          />
          <Search className="absolute left-3 top-1/2 size-4 -translate-y-1/2 text-muted-foreground" />
        </div>
      </div>

      {/* Apps grid */}
      <ScrollArea className="flex-1">
        <div className="flex flex-1 justify-center px-5 py-5">
          <div
            className="grid w-full max-w-[930px] justify-center"
            style={{
              gridTemplateColumns: "repeat(auto-fill, 90px)",
              gap: "25px",
            }}
          >
            {filteredApps.map((app) =>
              app.kind === "minapp" ? (
                <MinApp key={app.id} app={app} />
              ) : (
                <button
                  key={app.id}
                  className="group flex cursor-pointer flex-col items-center justify-center overflow-hidden border-0 bg-transparent"
                  style={{ minHeight: 85 }}
                  onClick={() => navigate(app.url)}
                >
                  <div className="relative flex items-center justify-center">
                    <img
                      src={app.logo}
                      alt={app.name}
                      className="h-[60px] w-[60px] rounded-[18px] object-cover shadow-[0_10px_24px_rgba(15,23,42,0.18)]"
                    />
                  </div>
                  <div className="mt-[5px] w-full max-w-[80px] select-none truncate text-center text-xs text-muted-foreground">
                    {app.name}
                  </div>
                </button>
              )
            )}
          </div>
        </div>
      </ScrollArea>
    </div>
  );
}
