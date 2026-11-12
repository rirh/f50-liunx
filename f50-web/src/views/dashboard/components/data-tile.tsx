import { KvList } from "@/components/f50/kv-list";
import { Badge } from "@/components/ui/badge";
import { Label } from "@/components/ui/label";
import { SweepShine } from "@/components/ui/sweep-shine";
import { Switch } from "@/components/ui/switch";
import { t } from "@/i18n";
import { errorMessage, formatDuration } from "@/lib/format";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { GlobeIcon } from "lucide-react";
import { toast } from "sonner";
import { Figure, Tile } from "./tile";
import { STATUS_KEY, setDataPower } from "../api";
import { DATA_HOLD_LABEL, DATA_PHASE_LABEL, label } from "../labels";
import type { DataInfo, OnOff, Part } from "../types";

function DataSwitch({ data }: { data: DataInfo }) {
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

export function DataTile({ part }: { part: Part<DataInfo> | undefined }) {
  return (
    <Tile
      title={t("dashboard.data.title")}
      icon={GlobeIcon}
      to="/cellular/data"
      part={part}
      action={(d) => <DataSwitch data={d} />}
    >
      {(d) => (
        <>
          <div className="flex items-center justify-between gap-3">
            <Figure
              value={label(DATA_PHASE_LABEL, d.state)}
              caption={
                d.state === "connected" && d.since_s != null
                  ? t("dashboard.data.connectedFor", {
                      time: formatDuration(d.since_s),
                    })
                  : d.apn || t("dashboard.data.defaultApn")
              }
            />
            <Badge
              variant={
                d.state === "connected"
                  ? "default"
                  : d.state === "failed"
                    ? "destructive"
                    : "outline"
              }
            >
              SIM{d.slot}
            </Badge>
          </div>
          {d.hold && (
            <p className="text-xs text-muted-foreground">
              {label(DATA_HOLD_LABEL, d.hold)}
            </p>
          )}
          {d.error && d.state !== "connected" && (
            <p className="truncate text-xs text-destructive" title={d.error}>
              {d.error}
            </p>
          )}
          <KvList
            dense
            items={[
              {
                label: t("dashboard.data.address"),
                value: d.addresses.length ? (
                  <span
                    className="block truncate font-mono"
                    title={d.addresses.join("\n")}
                  >
                    {d.addresses[0]}
                    {d.addresses.length > 1 && (
                      <span className="text-muted-foreground">
                        {" "}
                        +{d.addresses.length - 1}
                      </span>
                    )}
                  </span>
                ) : (
                  "—"
                ),
              },
              {
                label: t("dashboard.data.iface"),
                value: `${d.interface} (${d.device})`,
              },
            ]}
          />
        </>
      )}
    </Tile>
  );
}
