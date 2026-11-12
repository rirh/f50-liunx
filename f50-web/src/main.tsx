import React from "react";
import { HashRouter } from "react-router-dom";
import { LocaleGate } from "@/components/providers/locale-gate";
import { QueryProvider } from "@/components/providers/query-provider";
import { RouteProgress } from "@/components/providers/route-progress";
import { ThemeProvider } from "@/components/providers/theme-provider";
import { TooltipProvider } from "@/components/ui/tooltip";
import "@/index.css";
import { Provider } from "jotai";
import ReactDOM from "react-dom/client";

ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    <Provider>
      <ThemeProvider>
        <HashRouter>
          <RouteProgress />
          <QueryProvider>
            {/* the sidebar's collapsed menu and other tooltips need one provider above them */}
            <TooltipProvider>
              <LocaleGate />
            </TooltipProvider>
          </QueryProvider>
        </HashRouter>
      </ThemeProvider>
    </Provider>
  </React.StrictMode>,
);
