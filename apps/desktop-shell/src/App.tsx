import { Provider } from "react-redux";
import { PersistGate } from "redux-persist/integration/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { HashRouter } from "react-router-dom";
import { TooltipProvider } from "@/components/ui/tooltip";
import { ThemeProvider } from "@/components/ThemeProvider";
import { store, persistor } from "@/store";
import { AppShell } from "@/shell/AppShell";

const queryClient = new QueryClient({
  defaultOptions: {
    queries: {
      staleTime: 30_000,
      retry: 1,
    },
  },
});

export default function App() {
  return (
    <Provider store={store}>
      <PersistGate loading={null} persistor={persistor}>
        <QueryClientProvider client={queryClient}>
          <HashRouter>
            <ThemeProvider>
              <TooltipProvider delayDuration={300}>
                <AppShell />
              </TooltipProvider>
            </ThemeProvider>
          </HashRouter>
        </QueryClientProvider>
      </PersistGate>
    </Provider>
  );
}
