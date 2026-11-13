import { useState } from "react";
import { PageHeader } from "@/components/f50/page-header";
import { QueryState } from "@/components/f50/query-state";
import { t } from "@/i18n";
import { useQuery } from "@tanstack/react-query";
import { getSettingsInfo, infoKey } from "./api";
import { GeneralCard } from "./components/general-card";
import { type PowerAction, PowerCard } from "./components/power-card";
import { PowerWait } from "./components/power-wait";

/**
 * One General card (hostname, time zone, root password as read-only rows, each edited in its own dialog) beside the
 * power actions. Once a reboot or power-off starts, the page gives way to the wait screen.
 */
export default function Page() {
  const info = useQuery({ queryKey: infoKey, queryFn: getSettingsInfo });
  const [power, setPower] = useState<{
    action: PowerAction;
    since: number;
  } | null>(null);

  if (power) {
    return (
      <>
        <PageHeader
          title={t("system.settings.title")}
          description={
            power.action === "reboot"
              ? t("system.settings.rebooting")
              : t("system.settings.poweringOff")
          }
        />
        <PowerWait action={power.action} since={power.since} />
      </>
    );
  }
  return (
    <>
      <PageHeader
        title={t("system.settings.title")}
        description={t("system.settings.description")}
      />
      <QueryState query={info} skeletonRows={6}>
        {(d) => (
          <div className="grid items-start gap-6 lg:grid-cols-2">
            <GeneralCard info={d} />
            <PowerCard
              onStarted={(action) => setPower({ action, since: Date.now() })}
            />
          </div>
        )}
      </QueryState>
    </>
  );
}
