import OpenClawLogo from "@/assets/openclaw-logo.svg";
import type { MinAppType } from "@/types/minapp";

export function createOpenClawDashboardApp(url: string): MinAppType {
  return {
    id: "openclaw-dashboard",
    name: "OpenClaw",
    url,
    logo: OpenClawLogo,
    type: "custom",
    iconName: "Shell",
    description: "OpenClaw WebUI",
  };
}
