import { useRef } from "react";
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert";
import { Button } from "@/components/ui/button";
import { Skeleton } from "@/components/ui/skeleton";
import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from "@/components/ui/tooltip";
import { useLiveState, usePoll } from "@/hooks/use-live";
import { useStaggerIn } from "@/hooks/use-motion";
import { t } from "@/i18n";
import { errorMessage } from "@/lib/format";
import { cn } from "@/lib/utils";
import { useQuery } from "@tanstack/react-query";
import {
  CheckCircle2Icon,
  RefreshCwIcon,
  TriangleAlertIcon,
} from "lucide-react";
import { STATUS_KEY, getStatus } from "./api";
import { DeviceCard } from "./components/device-card";
import { ServiceList } from "./components/service-list";
import { Shortcuts } from "./components/shortcuts";
import { SystemCard } from "./components/system-card";
import { issuesOf } from "./health";
import type { DashboardStatus } from "./types";

const POLL_MS = 3000;

/** "All good", or how many things need a look (hover for which). */
function Health({ s }: { s: DashboardStatus }) {
  const issues = issuesOf(s);
  if (issues.length === 0)
    return (
      <span className="inline-flex items-center gap-1 rounded-full bg-[#34C759]/12 px-2.5 py-1 text-xs font-medium text-[#248A3D] dark:text-[#30D158]">
        <CheckCircle2Icon aria-hidden className="size-3.5" />
        {t("dashboard.health.ok")}
      </span>
    );
  const bad = issues.some((i) => i.bad);
  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <span
          tabIndex={0}
          className={cn(
            "inline-flex cursor-default items-center gap-1 rounded-full px-2.5 py-1 text-xs font-medium",
            bad
              ? "bg-[#FF3B30]/12 text-[#D70015] dark:text-[#FF453A]"
              : "bg-[#FF9500]/14 text-[#C93400] dark:text-[#FF9F0A]",
          )}
        >
          <TriangleAlertIcon aria-hidden className="size-3.5" />
          {issues.length === 1
            ? issues[0].text
            : t("dashboard.health.attention", { count: issues.length })}
        </span>
      </TooltipTrigger>
      {issues.length > 1 && (
        <TooltipContent side="bottom" className="flex flex-col gap-0.5">
          {issues.map((i) => (
            <span key={i.text}>{i.text}</span>
          ))}
        </TooltipContent>
      )}
    </Tooltip>
  );
}

/**
 * The device on one screen, each fact said once, with no page title (the breadcrumb names it). Left, the wider column:
 * the network card (carrier, speed, usage, signal), what the page is opened to see, and under it the device's own
 * resources. Right: the service list (data, hotspot, uplink, USB, messages, version: one row each, switches in place),
 * with the health and, while polling, a refresh button in its heading; then the shortcuts.
 */
export default function DashboardPage() {
  const query = useQuery({
    queryKey: STATUS_KEY,
    queryFn: getStatus,
    refetchInterval: usePoll(POLL_MS, "status"),
  });
  const live = useLiveState() === "live";
  const s = query.data;
  const page = useRef<HTMLDivElement>(null);
  useStaggerIn(page, !!s, "[data-reveal]");

  const aside = s && (
    <>
      <Health s={s} />
      {!live && (
        <Button
          variant="ghost"
          size="icon-sm"
          disabled={query.isFetching}
          aria-busy={query.isFetching}
          aria-label={t("common.action.refresh")}
          title={t("common.action.refresh")}
          onClick={() => query.refetch()}
        >
          <RefreshCwIcon className={cn(query.isFetching && "animate-spin")} />
        </Button>
      )}
    </>
  );

  return (
    <div ref={page} className="flex flex-col gap-4">
      {query.isError && (
        <Alert variant="destructive">
          <AlertTitle>
            {s ? t("dashboard.staleTitle") : t("dashboard.failedTitle")}
          </AlertTitle>
          <AlertDescription>{errorMessage(query.error)}</AlertDescription>
        </Alert>
      )}

      {s ? (
        <div className="grid grid-cols-1 items-start gap-4 lg:grid-cols-[minmax(0,1.25fr)_minmax(0,1fr)]">
          <div className="flex flex-col gap-4">
            <div data-reveal>
              <DeviceCard s={s} />
            </div>
            <div data-reveal>
              <SystemCard s={s} />
            </div>
          </div>
          <div className="flex flex-col gap-4">
            <div data-reveal>
              <ServiceList s={s} aside={aside} />
            </div>
            <div data-reveal>
              <Shortcuts />
            </div>
          </div>
        </div>
      ) : (
        query.isPending && (
          <div className="grid grid-cols-1 gap-4 lg:grid-cols-[minmax(0,1.25fr)_minmax(0,1fr)]">
            <div className="flex flex-col gap-4">
              <Skeleton className="h-[430px] rounded-2xl" />
              <Skeleton className="h-[300px] rounded-2xl" />
            </div>
            <div className="flex flex-col gap-4">
              <Skeleton className="h-[580px] rounded-2xl" />
              <Skeleton className="h-[260px] rounded-2xl" />
            </div>
          </div>
        )
      )}
    </div>
  );
}
