import { useState } from "react";
import { NavLink, useNavigate } from "react-router-dom";
import { LiveIndicator } from "@/components/f50/live-indicator";
import { UserAvatar } from "@/components/f50/user-avatar";
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from "@/components/ui/alert-dialog";
import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
} from "@/components/ui/collapsible";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import {
  Sidebar,
  SidebarContent,
  SidebarFooter,
  SidebarGroup,
  SidebarGroupContent,
  SidebarGroupLabel,
  SidebarHeader,
  SidebarMenu,
  SidebarMenuButton,
  SidebarMenuItem,
  useSidebar,
} from "@/components/ui/sidebar";
import { SweepShine } from "@/components/ui/sweep-shine";
import { t } from "@/i18n";
import { NAV, type NavGroup, type NavItem } from "@/layout/nav";
import { errorMessage } from "@/lib/format";
import { logout } from "@/lib/ubus";
import { reboot, switchToAndroid } from "@/views/dashboard/api";
import { AndroidDone } from "@/views/dashboard/components/android-done";
import { bootKey, getBoot } from "@/views/system/boot/api";
import { PowerWait } from "@/views/system/settings/components/power-wait";
import { useMutation, useQuery } from "@tanstack/react-query";
import {
  ArrowRightLeftIcon,
  ChevronRightIcon,
  EllipsisVerticalIcon,
  LogOutIcon,
  RotateCwIcon,
  RouterIcon,
} from "lucide-react";
import { toast } from "sonner";

function GroupSection({
  group,
  active,
}: {
  group: NavGroup;
  active: NavItem | null;
}) {
  const { isMobile, setOpenMobile } = useSidebar();
  return (
    <Collapsible className="group/collapsible" defaultOpen>
      <SidebarGroup className="py-1">
        <SidebarGroupLabel asChild className="h-7">
          <CollapsibleTrigger className="w-full cursor-pointer justify-between gap-2 text-left">
            <span className="min-w-0 flex-1 truncate">{t(group.title)}</span>
            <ChevronRightIcon className="ml-auto transition-transform group-data-[state=open]/collapsible:rotate-90" />
          </CollapsibleTrigger>
        </SidebarGroupLabel>
        <CollapsibleContent>
          <SidebarGroupContent>
            <SidebarMenu>
              {group.items.map((item) => {
                const on = active?.path === item.path;
                return (
                  <SidebarMenuItem key={item.path}>
                    <SidebarMenuButton
                      asChild
                      isActive={on}
                      tooltip={t(item.title)}
                    >
                      <NavLink
                        to={item.path}
                        end
                        aria-current={on ? "page" : undefined}
                        className="h-11 md:h-8"
                        onClick={() => isMobile && setOpenMobile(false)}
                      >
                        <item.icon aria-hidden />
                        <span>{t(item.title)}</span>
                      </NavLink>
                    </SidebarMenuButton>
                  </SidebarMenuItem>
                );
              })}
            </SidebarMenu>
          </SidebarGroupContent>
        </CollapsibleContent>
      </SidebarGroup>
    </Collapsible>
  );
}

type Ask = "reboot" | "android" | "logout";

/** After a reboot or the switch to Android: the console is going away, so it covers the whole page. */
function PowerOverlay({ e, at }: { e: "reboot" | "android"; at: number }) {
  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-background p-6">
      <div className="w-full max-w-lg">
        {e === "reboot" ? (
          <PowerWait action="reboot" since={at} />
        ) : (
          <AndroidDone />
        )}
      </div>
    </div>
  );
}

/** The signed-in user at the foot of the sidebar: a menu with reboot, switch to Android and sign-out, each asked first. */
function SessionIdentity({ username }: { username: string }) {
  const { isMobile } = useSidebar();
  const navigate = useNavigate();
  const [ask, setAsk] = useState<Ask | null>(null);
  const [done, setDone] = useState<{
    e: "reboot" | "android";
    at: number;
  } | null>(null);
  const boot = useQuery({
    queryKey: bootKey,
    queryFn: getBoot,
    staleTime: 60_000,
  });
  const out = useMutation({
    mutationFn: logout,
    onSettled: () => navigate("/login", { replace: true }),
  });
  const doReboot = useMutation({
    mutationFn: reboot,
    onSuccess: () => setDone({ e: "reboot", at: Date.now() }),
    onError: (e) => toast.error(errorMessage(e)),
    onSettled: () => setAsk(null),
  });
  const doAndroid = useMutation({
    mutationFn: switchToAndroid,
    onSuccess: () => setDone({ e: "android", at: Date.now() }),
    onError: (e) => toast.error(errorMessage(e)),
    onSettled: () => setAsk(null),
  });
  const busy = out.isPending || doReboot.isPending || doAndroid.isPending;
  // only refuse when the device says so; an unread boot state leaves the server to refuse
  const androidBlocked = boot.data ? !boot.data.writable : false;

  const dialog: Record<
    Ask,
    {
      title: string;
      body: string;
      confirm: string;
      run: () => void;
      pending: boolean;
    }
  > = {
    reboot: {
      title: t("dashboard.quick.rebootTitle"),
      body: t("dashboard.quick.rebootBody"),
      confirm: t("dashboard.quick.reboot"),
      run: () => doReboot.mutate(),
      pending: doReboot.isPending,
    },
    android: {
      title: t("dashboard.quick.androidTitle"),
      body: t("dashboard.quick.androidBody"),
      confirm: t("dashboard.quick.androidConfirm"),
      run: () => doAndroid.mutate(),
      pending: doAndroid.isPending,
    },
    logout: {
      title: t("app.shell.logoutTitle"),
      body: t("app.shell.logoutBody"),
      confirm: t("app.logout"),
      run: () => out.mutate(),
      pending: out.isPending,
    },
  };
  const d = ask ? dialog[ask] : null;

  return (
    <>
      <SidebarMenu>
        <SidebarMenuItem>
          <DropdownMenu>
            <DropdownMenuTrigger asChild>
              <SidebarMenuButton size="lg">
                <UserAvatar name={username} />
                <span className="grid min-w-0 flex-1 gap-0.5 text-left">
                  <span className="truncate font-medium">{username}</span>
                  <span className="truncate text-xs text-muted-foreground">
                    {t("app.shell.account")}
                  </span>
                </span>
                <EllipsisVerticalIcon className="ml-auto size-4" />
              </SidebarMenuButton>
            </DropdownMenuTrigger>
            <DropdownMenuContent
              align="end"
              side={isMobile ? "top" : "right"}
              className="min-w-48"
            >
              <DropdownMenuLabel className="truncate">
                {username}
              </DropdownMenuLabel>
              <DropdownMenuSeparator />
              <DropdownMenuItem
                disabled={busy}
                onSelect={() => setAsk("reboot")}
              >
                <RotateCwIcon />
                {t("dashboard.quick.reboot")}
              </DropdownMenuItem>
              <DropdownMenuItem
                variant="destructive"
                disabled={busy || androidBlocked}
                title={
                  androidBlocked
                    ? t("dashboard.quick.androidUnavailable")
                    : undefined
                }
                onSelect={() => setAsk("android")}
              >
                <ArrowRightLeftIcon />
                {t("dashboard.quick.android")}
              </DropdownMenuItem>
              <DropdownMenuSeparator />
              <DropdownMenuItem
                variant="destructive"
                disabled={busy}
                onSelect={() => setAsk("logout")}
              >
                <LogOutIcon />
                {t("app.logout")}
              </DropdownMenuItem>
            </DropdownMenuContent>
          </DropdownMenu>
        </SidebarMenuItem>
      </SidebarMenu>
      <AlertDialog
        open={ask !== null}
        onOpenChange={(v) => !busy && !v && setAsk(null)}
      >
        {d && (
          <AlertDialogContent>
            <AlertDialogHeader>
              <AlertDialogTitle>{d.title}</AlertDialogTitle>
              <AlertDialogDescription>{d.body}</AlertDialogDescription>
            </AlertDialogHeader>
            <AlertDialogFooter>
              <AlertDialogCancel disabled={busy}>
                {t("common.action.cancel")}
              </AlertDialogCancel>
              <AlertDialogAction
                variant="destructive"
                disabled={busy}
                aria-busy={d.pending}
                onClick={(e) => {
                  e.preventDefault();
                  d.run();
                }}
              >
                {d.pending ? <SweepShine>{d.confirm}</SweepShine> : d.confirm}
              </AlertDialogAction>
            </AlertDialogFooter>
          </AlertDialogContent>
        )}
      </AlertDialog>
      {done && <PowerOverlay e={done.e} at={done.at} />}
    </>
  );
}

/** one-template's sidebar: inset, off-canvas when collapsed, collapsible groups, the user at the foot. */
export function AppSidebar({
  active,
  username,
}: {
  active: NavItem | null;
  username: string;
}) {
  return (
    <Sidebar collapsible="offcanvas" variant="inset">
      <SidebarHeader>
        <div className="flex items-center justify-between gap-2 p-1.5">
          <NavLink
            to="/"
            className="flex min-w-0 items-center gap-2 rounded-md outline-hidden focus-visible:ring-2 focus-visible:ring-sidebar-ring"
          >
            <RouterIcon className="size-5 shrink-0 text-primary" />
            <span className="truncate text-base font-semibold">
              {t("app.brand")}
            </span>
          </NavLink>
          <LiveIndicator />
        </div>
      </SidebarHeader>
      <SidebarContent>
        {NAV.map((group) => (
          <GroupSection key={group.title} group={group} active={active} />
        ))}
      </SidebarContent>
      <SidebarFooter className="pb-[max(0.5rem,env(safe-area-inset-bottom))]">
        <SessionIdentity username={username} />
      </SidebarFooter>
    </Sidebar>
  );
}
