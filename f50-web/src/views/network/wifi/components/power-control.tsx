import { ConfirmButton } from "@/components/f50/confirm-button";
import { PendingButton } from "@/components/f50/pending-button";
import { errorMessage } from "@/lib/format";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { PowerIcon, PowerOffIcon } from "lucide-react";
import { toast } from "sonner";
import { powerWifi, wifiKeys } from "../api";
import type { WifiInfo, WifiPowerResult } from "../types";

const RESULT_TEXT: Record<WifiPowerResult["result"], string> = {
  pending: "已提交，Wi-Fi 正在重新加载",
  unchanged: "状态未变化",
  saved: "已保存，但重新加载失败：重启后生效",
};

/** The hotspot on or off. Turning it on is refused while the radio is the Wi-Fi client's. */
export function WifiPowerControl({ info }: { info: WifiInfo }) {
  const qc = useQueryClient();
  const power = useMutation({
    mutationFn: powerWifi,
    onSuccess: (r) => {
      if (r.result === "saved") toast.warning(RESULT_TEXT[r.result]);
      else toast.success(RESULT_TEXT[r.result]);
      qc.invalidateQueries({ queryKey: wifiKeys.info });
    },
    onError: (e) => toast.error(errorMessage(e)),
  });
  if (info.enabled) {
    return (
      <ConfirmButton
        size="sm"
        variant="outline"
        title="关闭热点？"
        description="所有通过 Wi-Fi 连接的设备都会断开。如果你正通过热点访问控制台，关闭后将无法继续访问，需要改用 USB 连接。"
        confirmText="关闭热点"
        pending={power.isPending}
        onConfirm={() => power.mutate(false)}
      >
        <PowerOffIcon data-icon="inline-start" />
        {power.isPending ? "关闭中" : "关闭热点"}
      </ConfirmButton>
    );
  }
  return (
    <PendingButton
      size="sm"
      pending={power.isPending}
      disabled={info.client_mode}
      title={
        info.client_mode ? "无线电正用于 Wi-Fi 上网，请先断开该网络" : undefined
      }
      onClick={() => power.mutate(true)}
    >
      <PowerIcon data-icon="inline-start" />
      {power.isPending ? "开启中" : "开启热点"}
    </PendingButton>
  );
}
