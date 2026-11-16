import React from "react";
import { BrowserRouter } from "react-router-dom";
import { LocaleGate } from "@/components/providers/locale-gate";
import { QueryProvider } from "@/components/providers/query-provider";
import { RouteProgress } from "@/components/providers/route-progress";
import { ThemeProvider } from "@/components/providers/theme-provider";
import { TooltipProvider } from "@/components/ui/tooltip";
import "@/index.css";
import { Provider } from "jotai";
import ReactDOM from "react-dom/client";

// The console's pages live at the site root (/, /sms, ...); its files stay under /f50/ (vite.config.ts). Addresses of the
// hash-router console it replaced (/f50/#/sms, /#/sms) and its old home (/f50/) are rewritten before the router reads them.
{
  const { pathname, search, hash } = window.location;
  if (hash.startsWith("#/")) window.history.replaceState(null, "", hash.slice(1));
  else if (/^\/f50(\/(index\.html)?)?$/.test(pathname))
    window.history.replaceState(null, "", `/${search}`);
}

// A lazy page's chunk that a newer deploy removed: uhttpd answers it with the console's index.html (error_page), the
// import fails, and one reload brings the new build. Once a minute at most, so a chunk that is really broken cannot loop.
window.addEventListener("vite:preloadError", (event) => {
  const key = "f50.preloadReload";
  const last = Number(sessionStorage.getItem(key) ?? 0);
  if (Date.now() - last < 60_000) return;
  sessionStorage.setItem(key, String(Date.now()));
  event.preventDefault();
  window.location.reload();
});

ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    <Provider>
      <ThemeProvider>
        <BrowserRouter>
          <RouteProgress />
          <QueryProvider>
            {/* the sidebar's collapsed menu and other tooltips need one provider above them */}
            <TooltipProvider>
              <LocaleGate />
            </TooltipProvider>
          </QueryProvider>
        </BrowserRouter>
      </ThemeProvider>
    </Provider>
  </React.StrictMode>,
);
