import { useState } from "react";
import { PageHeader } from "@/components/f50/page-header";
import { QueryState } from "@/components/f50/query-state";
import { useQuery } from "@tanstack/react-query";
import { getSettingsInfo, infoKey } from "./api";
import { HostnameForm } from "./components/hostname-form";
import { PasswordForm } from "./components/password-form";
import { type PowerAction, PowerCard } from "./components/power-card";
import { PowerWait } from "./components/power-wait";
import { TimezoneForm } from "./components/timezone-form";

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
          title="系统设置"
          description={
            power.action === "reboot" ? "设备正在重启" : "设备正在关机"
          }
        />
        <PowerWait action={power.action} since={power.since} />
      </>
    );
  }
  return (
    <>
      <PageHeader title="系统设置" description="主机名、时区、管理密码与电源" />
      <QueryState query={info} skeletonRows={6}>
        {(d) => (
          <div className="grid gap-6 lg:grid-cols-2">
            <HostnameForm current={d.hostname} />
            <TimezoneForm current={d.timezone} time={d.time} />
            <PasswordForm />
            <PowerCard
              onStarted={(action) => setPower({ action, since: Date.now() })}
            />
          </div>
        )}
      </QueryState>
    </>
  );
}
