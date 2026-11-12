import { KvList } from "@/components/f50/kv-list";
import { Badge } from "@/components/ui/badge";
import { SignalIcon } from "lucide-react";
import { SignalBars } from "./signal-bars";
import { Figure, Tile } from "./tile";
import { RAT_LABEL, SIM_STATE_LABEL } from "../labels";
import type { ModemStatus, Part, SimInfo } from "../types";

/** A reading older than this is shown as stale (the collector may be paused by a restart). */
const STALE_MS = 30_000;

function dbm(v: number | null | undefined, unit: string) {
  return v == null ? "—" : `${v} ${unit}`;
}

function SimLine({ sim }: { sim: Part<SimInfo> | undefined }) {
  if (!sim?.ok) return null;
  const s = sim.data;
  return (
    <span className="flex flex-wrap items-center gap-1">
      SIM{s.active}
      {s.state && (
        <span className="text-muted-foreground">
          · {SIM_STATE_LABEL[s.state]}
        </span>
      )}
      {s.reboot_required && (
        <Badge variant="outline">重启后切换到 SIM{s.default}</Badge>
      )}
    </span>
  );
}

export function CellularTile({
  modem,
  sim,
}: {
  modem: Part<ModemStatus> | undefined;
  sim: Part<SimInfo> | undefined;
}) {
  return (
    <Tile title="蜂窝网络" icon={SignalIcon} to="/cellular/signal" part={modem}>
      {(m) => {
        const stale = m.age_ms != null && m.age_ms > STALE_MS;
        return (
          <>
            <div className="flex items-center justify-between gap-3">
              <Figure
                value={m.operator ?? (m.registered ? "已注册" : "未注册")}
                caption={m.rat ? RAT_LABEL[m.rat] : "无网络"}
              />
              <SignalBars bars={m.bars} />
            </div>
            <div className="flex flex-wrap gap-1">
              <Badge variant={m.registered ? "secondary" : "destructive"}>
                {m.registered ? "已注册" : "未注册"}
              </Badge>
              {m.roaming && <Badge variant="outline">漫游</Badge>}
              {!m.radio_allowed && (
                <Badge variant="destructive">射频已关闭</Badge>
              )}
              {m.restarting && <Badge variant="outline">基带重启中</Badge>}
              {stale && <Badge variant="outline">读数已过期</Badge>}
            </div>
            <KvList
              items={[
                { label: "RSRP", value: dbm(m.signal?.rsrp, "dBm") },
                { label: "SINR", value: dbm(m.signal?.sinr, "dB") },
                { label: "RSRQ", value: dbm(m.signal?.rsrq, "dB") },
                {
                  label: "SIM 卡",
                  value: sim?.ok ? <SimLine sim={sim} /> : `SIM${m.slot}`,
                },
              ]}
            />
          </>
        );
      }}
    </Tile>
  );
}
