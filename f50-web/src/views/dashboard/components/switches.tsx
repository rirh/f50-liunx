import { useState } from "react";
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
import { Label } from "@/components/ui/label";
import { SweepShine } from "@/components/ui/sweep-shine";
import { Switch } from "@/components/ui/switch";
import { type Key, t } from "@/i18n";
import { errorMessage } from "@/lib/format";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { toast } from "sonner";
import { STATUS_KEY, setDataPower, setWifiPower } from "../api";
import type { DataInfo, OnOff, WifiInfo } from "../types";

/** Mobile data on/off: the dial follows (connecting, held) on its own. */
export function DataSwitch({ data }: { data: DataInfo }) {
  const qc = useQueryClient();
  const power = useMutation({
    mutationFn: (state: OnOff) => setDataPower(state),
    onSuccess: (r) =>
      toast.success(
        r.state === "on"
          ? t("dashboard.data.turnedOn")
          : t("dashboard.data.turnedOff"),
      ),
    onError: (e) => toast.error(errorMessage(e)),
    onSettled: () => {
      qc.invalidateQueries({ queryKey: STATUS_KEY });
      qc.invalidateQueries({ queryKey: ["f50.data"] });
    },
  });
  const on = power.isPending ? power.variables === "on" : data.state !== "down";
  return (
    <div className="flex items-center gap-2">
      <Label
        htmlFor="dash-data-power"
        className="text-[13px] text-muted-foreground"
      >
        {power.isPending ? (
          <SweepShine>{t("common.action.switching")}</SweepShine>
        ) : (
          <span className="sr-only">
            {on ? t("common.state.on") : t("common.state.off")}
          </span>
        )}
      </Label>
      <Switch
        id="dash-data-power"
        checked={on}
        disabled={power.isPending}
        aria-busy={power.isPending}
        aria-label={t("dashboard.data.toggle")}
        onCheckedChange={(v) => power.mutate(v ? "on" : "off")}
      />
    </div>
  );
}

const RESULT_TEXT = {
  pending: "dashboard.wifi.result.pending",
  unchanged: "dashboard.wifi.result.unchanged",
  saved: "dashboard.wifi.result.saved",
} as const satisfies Record<string, Key>;

/** The hotspot on/off; turning it off asks first (it may be the way to this page). */
export function WifiSwitch({ wifi }: { wifi: WifiInfo }) {
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
        className="text-[13px] text-muted-foreground"
      >
        {power.isPending ? (
          <SweepShine>{t("common.action.switching")}</SweepShine>
        ) : (
          <span className="sr-only">
            {on ? t("common.state.on") : t("common.state.off")}
          </span>
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
