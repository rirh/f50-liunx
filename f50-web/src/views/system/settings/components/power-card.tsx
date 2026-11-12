import { ConfirmButton } from "@/components/f50/confirm-button";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import { errorMessage } from "@/lib/format";
import { useMutation } from "@tanstack/react-query";
import { PowerIcon, RotateCwIcon } from "lucide-react";
import { toast } from "sonner";
import { poweroff, reboot } from "../api";

export type PowerAction = "reboot" | "poweroff";

/** Reboot and power-off: both answer first and act two seconds later. */
export function PowerCard({
  onStarted,
}: {
  onStarted: (a: PowerAction) => void;
}) {
  const doReboot = useMutation({
    mutationFn: reboot,
    onSuccess: () => onStarted("reboot"),
    onError: (e) => toast.error(errorMessage(e)),
  });
  const doPoweroff = useMutation({
    mutationFn: poweroff,
    onSuccess: () => onStarted("poweroff"),
    onError: (e) => toast.error(errorMessage(e)),
  });
  const busy = doReboot.isPending || doPoweroff.isPending;
  return (
    <Card>
      <CardHeader>
        <CardTitle>电源</CardTitle>
        <CardDescription>
          重启或关机会断开所有终端和移动数据连接
        </CardDescription>
      </CardHeader>
      <CardContent className="flex flex-wrap gap-2">
        <ConfirmButton
          title="重启设备？"
          description="设备将在 2 秒后重启，约 1–2 分钟内无法访问。重启后需要重新登录。"
          confirmText="重启"
          pending={doReboot.isPending}
          disabled={busy}
          onConfirm={() => doReboot.mutate()}
        >
          <RotateCwIcon data-icon="inline-start" />
          重启
        </ConfirmButton>
        <ConfirmButton
          title="关闭设备？"
          description="设备将在 2 秒后关机。关机后只能通过电源键重新开机，网页无法再唤醒它。"
          confirmText="关机"
          variant="outline"
          pending={doPoweroff.isPending}
          disabled={busy}
          onConfirm={() => doPoweroff.mutate()}
        >
          <PowerIcon data-icon="inline-start" />
          关机
        </ConfirmButton>
      </CardContent>
    </Card>
  );
}
