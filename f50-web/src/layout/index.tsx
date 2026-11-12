import { Outlet, useLocation, useNavigate } from "react-router-dom";
import { Tabs } from "radix-ui";
import { Toaster } from "sonner";
import { AnimatedSegmentedTabs } from "@/components/ui/animated-segmented-tabs";
import { BuildInfo } from "@/components/build-info";
import { ThemeToggleButton } from "@/components/theme/theme-toggle-button";
import { CookieConsentBanner } from "@/components/cookie-consent-banner";
import { AppUpdateChecker } from "@/components/providers/app-update-checker";
import { useTheme } from "@/hooks/use-theme";

const routes = [
  { label: "首页", value: "/" },
  { label: "关于", value: "/about" },
];

export function AppLayout() {
  const { resolvedTheme } = useTheme();
  const { pathname } = useLocation();
  const navigate = useNavigate();
  const activeRoute = pathname.replace(/\/+$/, "") || "/";
  const knownRoute = routes.some((route) => route.value === activeRoute);

  return (
    <>
      <div className="app-shell bg-muted/50 text-foreground">
        <div className="app-container mx-auto w-full max-w-3xl">
          <AnimatedSegmentedTabs
            label="主导航"
            options={routes}
            value={activeRoute}
            onValueChange={(value) => navigate(value)}
            activationMode="manual"
            className="grid grid-cols-[1fr_auto] items-start gap-x-4 gap-y-8 rounded-2xl border-0 bg-card p-5 shadow-none sm:p-8"
            listClassName="bg-muted/70"
            highlightClassName="shadow-none ring-0"
          >
            <ThemeToggleButton className="col-start-2 row-start-1" />
            {knownRoute ? (
              <Tabs.Content value={activeRoute} asChild>
                <main className="col-span-2 flex min-h-36 items-center py-4 sm:min-h-44">
                  <Outlet />
                </main>
              </Tabs.Content>
            ) : (
              <main className="col-span-2"><Outlet /></main>
            )}
          </AnimatedSegmentedTabs>
        </div>
        <footer className="absolute inset-x-6 bottom-[max(1.5rem,env(safe-area-inset-bottom))] flex justify-center">
          <BuildInfo />
        </footer>
      </div>
      <aside
        aria-label="站点通知"
        className="fixed right-2 bottom-[max(0.5rem,env(safe-area-inset-bottom))] z-50 flex max-h-[calc(100dvh-1rem)] w-[calc(100%-1rem)] max-w-[23rem] flex-col gap-2 overflow-y-auto p-1 sm:right-4 sm:bottom-[max(1rem,env(safe-area-inset-bottom))]"
      >
        <AppUpdateChecker />
        <CookieConsentBanner />
      </aside>
      <Toaster theme={resolvedTheme} position="top-right" richColors />
    </>
  );
}
