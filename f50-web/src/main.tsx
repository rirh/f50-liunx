import React from "react";
import { HashRouter } from "react-router-dom";
import { App } from "@/App";
import { QueryProvider } from "@/components/providers/query-provider";
import { RouteProgress } from "@/components/providers/route-progress";
import { ThemeProvider } from "@/components/providers/theme-provider";
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
            <App />
          </QueryProvider>
        </HashRouter>
      </ThemeProvider>
    </Provider>
  </React.StrictMode>,
);
