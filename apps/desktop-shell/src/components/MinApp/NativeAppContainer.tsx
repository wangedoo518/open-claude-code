import { CodePage } from "@/features/code/CodePage";
import { OpenClawPage } from "@/features/workbench/OpenClawPage";
import type { MinAppType } from "@/types/minapp";

interface NativeAppContainerProps {
  app: MinAppType;
}

/**
 * Container for built-in (native React) apps.
 *
 * Detects the `warwolf://` protocol URL and renders the corresponding
 * React component. External URL apps would use an iframe instead.
 *
 * This replaces cherry-studio's WebviewContainer for built-in apps.
 */
export function NativeAppContainer({ app }: NativeAppContainerProps) {
  const route = app.url.replace("warwolf://", "");

  switch (route) {
    case "code":
      return (
        <CodePage
          tabId={`app-code-${app.id}`}
          showSessionSidebar={true}
          syncTabState={false}
        />
      );
    case "openclaw":
      return <OpenClawPage />;
    default:
      return (
        <div className="flex h-full items-center justify-center text-muted-foreground">
          Unknown app: {app.url}
        </div>
      );
  }
}

/**
 * Container for external URL apps rendered via iframe.
 * Used for custom apps that load external web pages.
 */
export function IframeAppContainer({ app }: NativeAppContainerProps) {
  return (
    <iframe
      src={app.url}
      className="h-full w-full border-0"
      sandbox="allow-scripts allow-same-origin allow-forms allow-popups"
      title={app.name}
      data-minapp-id={app.id}
    />
  );
}

/**
 * Resolves the correct container for a MinApp based on its URL protocol.
 */
export function AppContainer({ app }: NativeAppContainerProps) {
  if (app.url.startsWith("warwolf://")) {
    return <NativeAppContainer app={app} />;
  }
  return <IframeAppContainer app={app} />;
}
