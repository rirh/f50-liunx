import { Suspense } from "react";
import { Navigate, Outlet, useLocation, useNavigate } from "react-router-dom";
import { BuildInfo } from "@/components/build-info";
import { LanguageSwitcher } from "@/components/f50/language-switcher";
import { LiveIndicator } from "@/components/f50/live-indicator";
import { AppUpdateChecker } from "@/components/providers/app-update-checker";
import { LiveProvider } from "@/components/providers/live-provider";
import { ThemeToggleButton } from "@/components/theme/theme-toggle-button";
import { Button } from "@/components/ui/button";
import { Separator } from "@/components/ui/separator";
import {
  SidebarInset,
  SidebarProvider,
  SidebarTrigger,
} from "@/components/ui/sidebar";
import { SweepShine } from "@/components/ui/sweep-shine";
import { useSession } from "@/hooks/use-session";
import { useTheme } from "@/hooks/use-theme";
import { t } from "@/i18n";
import { AppSidebar } from "@/layout/app-sidebar";
import { titleOf } from "@/layout/nav";
import { logout } from "@/lib/ubus";
import { useMutation } from "@tanstack/react-query";
import { LogOutIcon } from "lucide-react";
import { Toaster } from "sonner";

export function PageFallback() {
  return (
    <div className="py-16 text-center text-muted-foreground">
      <SweepShine>{t("app.loading")}</SweepShine>
    </div>
  );
}

/** Notices and toasts, shared by the login page and the app shell. */
export function GlobalChrome() {
  const { resolvedTheme } = useTheme();
  return (
    <>
      <aside
        aria-label={t("app.notices")}
        className="fixed right-2 bottom-[max(0.5rem,env(safe-area-inset-bottom))] z-50 flex max-h-[calc(100dvh-1rem)] w-[calc(100%-1rem)] max-w-[23rem] flex-col gap-2 overflow-y-auto p-1 sm:right-4"
      >
        <AppUpdateChecker />
      </aside>
      <Toaster theme={resolvedTheme} position="top-right" richColors />
    </>
  );
}

/** The signed-in shell: the sidebar, a header with the page title, theme and logout, and the page. */
export function AppLayout() {
  const session = useSession();
  const { pathname } = useLocation();
  const navigate = useNavigate();
  const out = useMutation({
    mutationFn: logout,
    onSettled: () => navigate("/login", { replace: true }),
  });

  if (!session)
    return <Navigate to="/login" replace state={{ from: pathname }} />;

  return (
    <LiveProvider>
      <SidebarProvider>
        <AppSidebar />
        <SidebarInset>
          <header className="sticky top-0 z-10 flex h-14 shrink-0 items-center gap-2 border-b bg-background/80 px-4 backdrop-blur">
            <SidebarTrigger />
            <Separator orientation="vertical" className="h-4" />
            <span className="truncate text-sm font-medium">
              {titleOf(pathname)}
            </span>
            <div className="ml-auto flex items-center gap-2">
              <LiveIndicator />
              <LanguageSwitcher />
              <span className="hidden text-sm text-muted-foreground sm:inline">
                {session.username}
              </span>
              <ThemeToggleButton />
              <Button
                variant="ghost"
                size="sm"
                disabled={out.isPending}
                aria-busy={out.isPending}
                onClick={() => out.mutate()}
              >
                <LogOutIcon data-icon="inline-start" />
                {out.isPending ? (
                  <SweepShine>{t("app.logout")}</SweepShine>
                ) : (
                  t("app.logout")
                )}
              </Button>
            </div>
          </header>
          <main className="mx-auto flex w-full max-w-6xl flex-col gap-6 p-4 sm:p-6">
            <Suspense fallback={<PageFallback />}>
              <Outlet />
            </Suspense>
          </main>
          <footer className="flex justify-center pb-4">
            <BuildInfo />
          </footer>
        </SidebarInset>
        <GlobalChrome />
      </SidebarProvider>
    </LiveProvider>
  );
}
