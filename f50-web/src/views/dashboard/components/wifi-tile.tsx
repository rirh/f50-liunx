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
import { type Key, t } from "@/i18n";
import { errorMessage } from "@/lib/format";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { WifiIcon } from "lucide-react";
import { toast } from "sonner";
import { Figure, Tile } from "./tile";
import { STATUS_KEY, setWifiPower } from "../api";
import { SECURITY_LABEL, label } from "../labels";
import type { OnOff, Part, WifiInfo } from "../types";

const RESULT_TEXT = {
  pending: "dashboard.wifi.result.pending",
  unchanged: "dashboard.wifi.result.unchanged",
  saved: "dashboard.wifi.result.saved",
} as const satisfies Record<string, Key>;

function WifiSwitch({ wifi }: { wifi: WifiInfo }) {
  const qc = useQueryClient();
  const [confirmOff, setConfirmOff] = useState(false);
  const power = useMutation({
    mutationFn: (state: OnOff) => setWifiPower(state),
    onSuccess: (r) =>
      toast.success(
        RESULT_TEXT[r.result]
          ? t(RESULT_TEXT[r.result])
          : t("dashboard.wifi.done"),
      ),
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
        {power.isPending ? (
          <SweepShine>{t("common.action.switching")}</SweepShine>
        ) : on ? (
          t("common.state.on")
        ) : (
          t("common.state.off")
        )}
      </Label>
      <Switch
        id="dash-wifi-power"
        checked={on}
        disabled={power.isPending || wifi.client_mode}
        aria-busy={power.isPending}
        aria-label={t("dashboard.wifi.toggle")}
        onCheckedChange={(v) => (v ? power.mutate("on") : setConfirmOff(true))}
      />
      <AlertDialog open={confirmOff} onOpenChange={setConfirmOff}>
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>
              {t("dashboard.wifi.confirmTitle")}
            </AlertDialogTitle>
            <AlertDialogDescription>
              {t("dashboard.wifi.confirmBody")}
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel>{t("common.action.cancel")}</AlertDialogCancel>
            <AlertDialogAction
              variant="destructive"
              onClick={() => power.mutate("off")}
            >
              {t("dashboard.wifi.confirmAction")}
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
      title={t("dashboard.wifi.title")}
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
                w.enabled
                  ? t("dashboard.wifi.clients", { count: w.clients.length })
                  : t("dashboard.wifi.off")
              }
            />
            <Badge variant="secondary">{w.band} GHz</Badge>
          </div>
          {w.client_mode && (
            <p className="text-xs text-muted-foreground">
              {t("dashboard.wifi.clientMode")}
            </p>
          )}
          <KvList
            dense
            items={[
              {
                label: t("dashboard.wifi.channel"),
                value:
                  w.channel_live ??
                  (w.channel === "auto" ? t("common.state.auto") : w.channel),
              },
              {
                label: t("dashboard.wifi.security"),
                value: label(SECURITY_LABEL, w.security),
              },
            ]}
          />
        </>
      )}
    </Tile>
  );
}
