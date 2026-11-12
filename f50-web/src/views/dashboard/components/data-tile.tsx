import { KvList } from "@/components/f50/kv-list";
import { Badge } from "@/components/ui/badge";
import { Label } from "@/components/ui/label";
import { SweepShine } from "@/components/ui/sweep-shine";
import { Switch } from "@/components/ui/switch";
import { errorMessage, formatDuration } from "@/lib/format";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { GlobeIcon } from "lucide-react";
import { toast } from "sonner";
import { Figure, Tile } from "./tile";
import { STATUS_KEY, setDataPower } from "../api";
import { DATA_HOLD_LABEL, DATA_PHASE_LABEL } from "../labels";
import type { DataInfo, OnOff, Part } from "../types";

function DataSwitch({ data }: { data: DataInfo }) {
  const qc = useQueryClient();
  const power = useMutation({
    mutationFn: (state: OnOff) => setDataPower(state),
    onSuccess: (r) =>
      toast.success(r.state === "on" ? "已开启移动数据" : "已关闭移动数据"),
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
        {power.isPending ? <SweepShine>切换中</SweepShine> : on ? "开" : "关"}
      </Label>
      <Switch
        id="dash-data-power"
        checked={on}
        disabled={power.isPending}
        aria-busy={power.isPending}
        aria-label="移动数据开关"
        onCheckedChange={(v) => power.mutate(v ? "on" : "off")}
      />
    </div>
  );
}

export function DataTile({ part }: { part: Part<DataInfo> | undefined }) {
  return (
    <Tile
      title="数据连接"
      icon={GlobeIcon}
      to="/cellular/data"
      part={part}
      action={(d) => <DataSwitch data={d} />}
    >
      {(d) => (
        <>
          <div className="flex items-center justify-between gap-3">
            <Figure
              value={DATA_PHASE_LABEL[d.state] ?? d.state}
              caption={
                d.state === "connected" && d.since_s != null
                  ? `已连接 ${formatDuration(d.since_s)}`
                  : d.apn || "默认 APN"
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
            <p className="text-sm text-muted-foreground">
              {DATA_HOLD_LABEL[d.hold] ?? d.hold}
            </p>
          )}
          {d.error && d.state !== "connected" && (
            <p className="text-sm text-destructive">{d.error}</p>
          )}
          <KvList
            items={[
              {
                label: "地址",
                value: d.addresses.length ? (
                  <span className="flex flex-col font-mono text-xs">
                    {d.addresses.map((a) => (
                      <span key={a} className="truncate">
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
                  <span className="font-mono text-xs">{d.dns.join(", ")}</span>
                ) : (
                  "—"
                ),
              },
              { label: "接口", value: `${d.interface} (${d.device})` },
            ]}
          />
        </>
      )}
    </Tile>
  );
}
