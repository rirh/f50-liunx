import { ConfirmButton } from "@/components/f50/confirm-button";
import { PendingButton } from "@/components/f50/pending-button";
import { t, type Key } from "@/i18n";
import { errorMessage } from "@/lib/format";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { PowerIcon, PowerOffIcon } from "lucide-react";
import { toast } from "sonner";
import { powerWifi, wifiKeys } from "../api";
import type { WifiInfo, WifiPowerResult } from "../types";

const RESULT_TEXT = {
  pending: "network.wifi.power.result.pending",
  unchanged: "network.wifi.power.result.unchanged",
  saved: "network.wifi.power.result.saved",
} satisfies Record<WifiPowerResult["result"], Key>;

/** The hotspot on or off. Turning it on is refused while the radio is the Wi-Fi client's. */
export function WifiPowerControl({ info }: { info: WifiInfo }) {
  const qc = useQueryClient();
  const power = useMutation({
    mutationFn: powerWifi,
    onSuccess: (r) => {
      if (r.result === "saved") toast.warning(t(RESULT_TEXT[r.result]));
      else toast.success(t(RESULT_TEXT[r.result]));
      qc.invalidateQueries({ queryKey: wifiKeys.info });
    },
    onError: (e) => toast.error(errorMessage(e)),
  });
  if (info.enabled) {
    return (
      <ConfirmButton
        size="sm"
        variant="outline"
        title={t("network.wifi.power.offTitle")}
        description={t("network.wifi.power.offDescription")}
        confirmText={t("network.wifi.power.off")}
        pending={power.isPending}
        onConfirm={() => power.mutate(false)}
      >
        <PowerOffIcon data-icon="inline-start" />
        {power.isPending
          ? t("network.wifi.power.turningOff")
          : t("network.wifi.power.off")}
      </ConfirmButton>
    );
  }
  return (
    <PendingButton
      size="sm"
      pending={power.isPending}
      disabled={info.client_mode}
      title={info.client_mode ? t("network.wifi.power.radioBusy") : undefined}
      onClick={() => power.mutate(true)}
    >
      <PowerIcon data-icon="inline-start" />
      {power.isPending
        ? t("network.wifi.power.turningOn")
        : t("network.wifi.power.on")}
    </PendingButton>
  );
}
