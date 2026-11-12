import { ConfirmButton } from "@/components/f50/confirm-button";
import { KvList } from "@/components/f50/kv-list";
import { PendingButton } from "@/components/f50/pending-button";
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert";
import { Badge } from "@/components/ui/badge";
import {
  Card,
  CardAction,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import { reportFormError } from "@/lib/form-error";
import { formatDuration } from "@/lib/format";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { toast } from "sonner";
import { dataKeys, setDataPower } from "../api";
import type { DataHold, DataPhase, DataStatus } from "../types";

const PHASE: Record<
  DataPhase,
  {
    label: string;
    variant: "default" | "secondary" | "destructive" | "outline";
  }
> = {
  connected: { label: "已连接", variant: "default" },
  connecting: { label: "连接中", variant: "outline" },
  held: { label: "已暂停", variant: "secondary" },
  failed: { label: "连接失败", variant: "destructive" },
  down: { label: "已断开", variant: "secondary" },
};

const HOLD: Record<DataHold, string> = {
  roaming: "正在漫游且未允许数据漫游",
  "sim-missing": "当前卡槽没有 SIM 卡",
  "radio-off": "射频已关闭（飞行模式或该卡射频关闭）",
};

export function StatusCard({ d }: { d: DataStatus }) {
  const qc = useQueryClient();
  const power = useMutation({
    mutationFn: setDataPower,
    onSuccess: (r) => {
      toast.success(r.state === "on" ? "已开启移动数据" : "已关闭移动数据");
      qc.invalidateQueries({ queryKey: dataKeys.get });
    },
    onError: (e) => reportFormError(e),
  });
  const on = d.state !== "down";
  const phase = PHASE[d.state];
  return (
    <Card>
      <CardHeader>
        <CardTitle>连接状态</CardTitle>
        <CardDescription>
          接口 {d.interface} · SIM{d.slot}
        </CardDescription>
        <CardAction>
          <Badge variant={phase.variant}>{phase.label}</Badge>
        </CardAction>
      </CardHeader>
      <CardContent className="flex flex-col gap-4">
        {d.hold && (
          <Alert>
            <AlertTitle>数据已暂停</AlertTitle>
            <AlertDescription>{HOLD[d.hold]}</AlertDescription>
          </Alert>
        )}
        {d.state === "failed" && d.error && (
          <Alert variant="destructive">
            <AlertTitle>拨号失败</AlertTitle>
            <AlertDescription>{d.error}</AlertDescription>
          </Alert>
        )}
        <KvList
          items={[
            { label: "持续时间", value: formatDuration(d.since_s) },
            {
              label: "网络设备",
              value: <span className="font-mono">{d.device}</span>,
            },
            {
              label: "地址",
              value: d.addresses.length ? (
                <span className="flex flex-col font-mono">
                  {d.addresses.map((a) => (
                    <span key={a} className="break-all">
                      {a}
                    </span>
                  ))}
                </span>
              ) : (
                "—"
              ),
            },
            {
              label: "DNS",
              value: d.dns?.length ? (
                <span className="font-mono break-all">{d.dns.join(", ")}</span>
              ) : (
                "—"
              ),
            },
            { label: "IPv6 中继", value: d.relay ? "是" : "否" },
            { label: "APN 绑定 ICCID", value: d.iccid_bound ? "是" : "否" },
          ]}
        />
        <div className="flex flex-wrap gap-2">
          {on ? (
            <ConfirmButton
              title="关闭移动数据？"
              description="蜂窝数据连接将断开，直到重新开启或设备重启。"
              confirmText="关闭"
              pending={power.isPending}
              onConfirm={() => power.mutate(false)}
            >
              关闭移动数据
            </ConfirmButton>
          ) : (
            <PendingButton
              pending={power.isPending}
              onClick={() => power.mutate(true)}
            >
              开启移动数据
            </PendingButton>
          )}
        </div>
      </CardContent>
    </Card>
  );
}
