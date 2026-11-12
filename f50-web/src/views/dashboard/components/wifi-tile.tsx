import { useState } from "react";
import { KvList } from "@/components/f50/kv-list";
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
import { Badge } from "@/components/ui/badge";
import { Label } from "@/components/ui/label";
import { SweepShine } from "@/components/ui/sweep-shine";
import { Switch } from "@/components/ui/switch";
import { errorMessage } from "@/lib/format";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { WifiIcon } from "lucide-react";
import { toast } from "sonner";
import { Figure, Tile } from "./tile";
import { STATUS_KEY, setWifiPower } from "../api";
import { SECURITY_LABEL } from "../labels";
import type { OnOff, Part, WifiInfo } from "../types";

const RESULT_TEXT = {
  pending: "已应用，热点正在重新加载",
  unchanged: "热点状态未变化",
  saved: "已保存，重启或重新加载 Wi-Fi 后生效",
} as const;

function WifiSwitch({ wifi }: { wifi: WifiInfo }) {
  const qc = useQueryClient();
  const [confirmOff, setConfirmOff] = useState(false);
  const power = useMutation({
    mutationFn: (state: OnOff) => setWifiPower(state),
    onSuccess: (r) => toast.success(RESULT_TEXT[r.result] ?? "已完成"),
    onError: (e) => toast.error(errorMessage(e)),
    onSettled: () => {
      qc.invalidateQueries({ queryKey: STATUS_KEY });
      qc.invalidateQueries({ queryKey: ["f50.wifi"] });
    },
  });
  const on = power.isPending ? power.variables === "on" : wifi.enabled;
  return (
    <div className="flex items-center gap-2">
      <Label
        htmlFor="dash-wifi-power"
        className="text-xs text-muted-foreground"
      >
        {power.isPending ? <SweepShine>切换中</SweepShine> : on ? "开" : "关"}
      </Label>
      <Switch
        id="dash-wifi-power"
        checked={on}
        disabled={power.isPending || wifi.client_mode}
        aria-busy={power.isPending}
        aria-label="Wi-Fi 热点开关"
        onCheckedChange={(v) => (v ? power.mutate("on") : setConfirmOff(true))}
      />
      <AlertDialog open={confirmOff} onOpenChange={setConfirmOff}>
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>关闭 Wi-Fi 热点？</AlertDialogTitle>
            <AlertDialogDescription>
              所有通过 Wi-Fi 连接的设备都会断开。如果你正通过 Wi-Fi
              访问此页面，关闭后需改用 USB 或其他方式重新连接。
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel>取消</AlertDialogCancel>
            <AlertDialogAction
              variant="destructive"
              onClick={() => power.mutate("off")}
            >
              关闭热点
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </div>
  );
}

export function WifiTile({ part }: { part: Part<WifiInfo> | undefined }) {
  return (
    <Tile
      title="Wi-Fi 热点"
      icon={WifiIcon}
      to="/network/wifi"
      part={part}
      action={(w) => <WifiSwitch wifi={w} />}
    >
      {(w) => (
        <>
          <div className="flex items-center justify-between gap-3">
            <Figure
              value={w.ssid ?? "—"}
              caption={
                w.enabled ? `${w.clients.length} 台设备在线` : "热点已关闭"
              }
            />
            <Badge variant="secondary">{w.band} GHz</Badge>
          </div>
          {w.client_mode && (
            <p className="text-sm text-muted-foreground">
              无线芯片正用作 Wi-Fi 上行，热点暂不可用
            </p>
          )}
          <KvList
            items={[
              {
                label: "信道",
                value:
                  w.channel_live ?? (w.channel === "auto" ? "自动" : w.channel),
              },
              {
                label: "加密",
                value: SECURITY_LABEL[w.security] ?? w.security,
              },
              { label: "隐藏 SSID", value: w.hidden ? "是" : "否" },
            ]}
          />
        </>
      )}
    </Tile>
  );
}
