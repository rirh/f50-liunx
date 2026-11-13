import {
  type CSSProperties,
  Fragment,
  Suspense,
  useEffect,
  useState,
} from "react";
import {
  Link,
  Navigate,
  Outlet,
  useLocation,
  useNavigate,
} from "react-router-dom";
import { useBuildLog } from "@/components/build-info";
import { LanguageSwitcher } from "@/components/f50/language-switcher";
import { AppUpdateChecker } from "@/components/providers/app-update-checker";
import { LiveProvider } from "@/components/providers/live-provider";
import { ThemeToggleButton } from "@/components/theme/theme-toggle-button";
import {
  Breadcrumb,
  BreadcrumbItem,
  BreadcrumbLink,
  BreadcrumbList,
  BreadcrumbPage,
  BreadcrumbSeparator,
} from "@/components/ui/breadcrumb";
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
import { NAV, type NavItem, locate } from "@/layout/nav";
import { PageTransition } from "@/layout/page-transition";
import { RouteTags } from "@/layout/route-tags";
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
  useBuildLog();
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

const TABS_KEY = "f50-tabs";
const ALL = NAV.flatMap((g) => g.items);

function readTabs(): string[] {
  try {
    const v = JSON.parse(sessionStorage.getItem(TABS_KEY) ?? "[]");
    if (Array.isArray(v)) {
      const known = v.filter(
        (p): p is string =>
          typeof p === "string" && ALL.some((i) => i.path === p),
      );
      if (known.length) return known;
    }
  } catch {
    // unreadable: start again from the dashboard
  }
  return ["/"];
}

/** Home › group › page; the dashboard is the home itself. */
function Crumbs({ pathname }: { pathname: string }) {
  const at = locate(pathname);
  const home = ALL[0];
  const trail: { label: string; to?: string }[] =
    !at || at.item.path === home.path
      ? [{ label: t(home.title) }]
      : [
          { label: t("app.shell.home"), to: home.path },
          { label: t(at.group.title) },
          { label: t(at.item.title) },
        ];
  return (
    <Breadcrumb className="min-w-0">
      <BreadcrumbList className="flex-nowrap">
        {trail.map((c, i) => (
          <Fragment key={i}>
            {i > 0 && <BreadcrumbSeparator className="max-sm:hidden" />}
            <BreadcrumbItem
              className={i < trail.length - 1 ? "hidden sm:inline-flex" : ""}
            >
              {i === trail.length - 1 ? (
                <BreadcrumbPage className="truncate font-medium">
                  {c.label}
                </BreadcrumbPage>
              ) : c.to ? (
                <BreadcrumbLink asChild>
                  <Link to={c.to}>{c.label}</Link>
                </BreadcrumbLink>
              ) : (
                <span>{c.label}</span>
              )}
            </BreadcrumbItem>
          </Fragment>
        ))}
      </BreadcrumbList>
    </Breadcrumb>
  );
}

/**
 * The signed-in shell, as one-template lays it out: an inset sidebar (groups, the user at the foot), and beside it a
 * header (sidebar toggle, a full-height divider, the breadcrumb, live state, language, theme), the tabs of the pages
 * opened in this visit, and the page in its own scroll area.
 */
export function AppLayout() {
  const session = useSession();
  const { pathname } = useLocation();
  const navigate = useNavigate();
  const active = locate(pathname)?.item ?? null;
  const [tabs, setTabs] = useState<string[]>(readTabs);

  // a page reached by link or address gets its tab during render (React's "adjust state on a prop change")
  if (active && !tabs.includes(active.path)) setTabs([...tabs, active.path]);
  useEffect(() => {
    try {
      sessionStorage.setItem(TABS_KEY, JSON.stringify(tabs));
    } catch {
      // storage unavailable: the tabs last for this page load
    }
  }, [tabs]);

  if (!session)
    return <Navigate to="/login" replace state={{ from: pathname }} />;

  const items = tabs
    .map((p) => ALL.find((i) => i.path === p))
    .filter((i): i is NavItem => !!i);
  const close = (item: NavItem) => {
    if (items.length <= 1) return;
    const next = items.filter((i) => i.path !== item.path);
    setTabs(next.map((i) => i.path));
    if (active?.path === item.path) navigate(next.at(-1)!.path);
  };

  return (
    <LiveProvider>
      <SidebarProvider
        className="h-svh overflow-hidden"
        style={
          {
            "--sidebar-width": "calc(var(--spacing) * 60)",
            "--header-height": "3rem",
          } as CSSProperties
        }
      >
        <AppSidebar active={active} username={session.username} />
        <SidebarInset className="h-svh min-h-0 overflow-hidden md:h-[calc(100svh-1rem)]">
          <header className="flex h-(--header-height) shrink-0 items-center border-b bg-background">
            <div className="flex h-full w-full min-w-0 items-center gap-3 px-3 lg:px-4">
              <SidebarTrigger className="-ml-1 size-8" />
              <div aria-hidden className="h-full w-px shrink-0 bg-border" />
              <Crumbs pathname={pathname} />
              <div className="ml-auto flex shrink-0 items-center gap-1.5">
                <LanguageSwitcher />
                <ThemeToggleButton variant="outline" />
              </div>
            </div>
          </header>
          <RouteTags
            active={active}
            items={items}
            onClose={close}
            onSelect={(i) => navigate(i.path)}
          />
          <main className="@container/main flex min-h-0 flex-1 flex-col overflow-hidden bg-background">
            <PageTransition routeKey={pathname}>
              {active?.fullBleed ? (
                <div className="flex min-h-0 flex-1 flex-col overflow-hidden">
                  <Suspense fallback={<PageFallback />}>
                    <Outlet />
                  </Suspense>
                </div>
              ) : (
                <div className="mx-auto flex w-full max-w-6xl flex-col gap-6 p-4 sm:p-6">
                  <Suspense fallback={<PageFallback />}>
                    <Outlet />
                  </Suspense>
                </div>
              )}
            </PageTransition>
          </main>
        </SidebarInset>
        <GlobalChrome />
      </SidebarProvider>
    </LiveProvider>
  );
}
