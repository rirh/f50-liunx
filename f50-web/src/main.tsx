import { Provider } from "jotai";
import React from "react";
import ReactDOM from "react-dom/client";
import { BrowserRouter } from "react-router-dom";
import { QueryProvider } from "@/components/providers/query-provider";
import { ThemeProvider } from "@/components/providers/theme-provider";
import { App } from "@/App";
import { RouteProgress } from "@/components/providers/route-progress";
import "@/index.css";

ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    <Provider>
      <ThemeProvider>
        <BrowserRouter>
          <RouteProgress />
          <QueryProvider>
            <App />
          </QueryProvider>
        </BrowserRouter>
      </ThemeProvider>
    </Provider>
  </React.StrictMode>
);
