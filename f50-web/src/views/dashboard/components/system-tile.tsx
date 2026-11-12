import { useEffect, useRef, useState } from "react";
import { KvList } from "@/components/f50/kv-list";
import { Badge } from "@/components/ui/badge";
import { Progress } from "@/components/ui/progress";
import { formatBytes, formatDuration } from "@/lib/format";
import { CpuIcon } from "lucide-react";
import { Figure, Tile } from "./tile";
import type { Part, SystemStatusInfo } from "../types";

type Load = SystemStatusInfo["load"];

/** CPU use between two polls (the reply carries `/proc/stat`'s counters, not a percentage). */
function useCpuPercent(load: Load) {
  const prev = useRef<Load>(null);
  const [pct, setPct] = useState<number | null>(null);
  useEffect(() => {
    const p = prev.current;
    prev.current = load;
    if (!p || !load) return;
    const dt = load.total - p.total;
    const di = load.idle - p.idle;
    if (dt > 0) setPct(Math.max(0, Math.min(100, (1 - di / dt) * 100)));
  }, [load]);
  return pct;
}

function SystemBody({ s }: { s: SystemStatusInfo }) {
  const cpu = useCpuPercent(s.load);
  const { total_kb, avail_kb } = s.memory;
  const memPct =
    total_kb && avail_kb != null
      ? ((total_kb - avail_kb) / total_kb) * 100
      : null;
  const temp = s.temperatures.hottest ?? s.temperatures.cpu;
  const mhz = s.load?.freqs
    .map((f) => (f.cur != null ? Math.round(f.cur / 1000) : null))
    .filter((f) => f != null);
  return (
    <>
      <div className="grid grid-cols-3 gap-3">
        <Figure
          value={cpu != null ? `${cpu.toFixed(0)}%` : "—"}
          caption={`CPU · ${s.load?.n ?? "—"} 核`}
        />
        <Figure
          value={memPct != null ? `${memPct.toFixed(0)}%` : "—"}
          caption="内存"
        />
        <Figure
          value={temp != null ? `${temp.toFixed(1)}℃` : "—"}
          caption="最高温度"
        />
      </div>
      <div className="flex flex-col gap-2">
        <Progress value={cpu ?? 0} aria-label="CPU 使用率" />
        <Progress value={memPct ?? 0} aria-label="内存使用率" />
      </div>
      {s.heat_alarm && <Badge variant="destructive">温度过高，已降频</Badge>}
      <KvList
        items={[
          { label: "运行时间", value: formatDuration(s.uptime) },
          {
            label: "内存",
            value:
              total_kb && avail_kb != null
                ? `${formatBytes((total_kb - avail_kb) * 1024)} / ${formatBytes(total_kb * 1024)}`
                : "—",
          },
          {
            label: "频率",
            value: mhz?.length ? `${mhz.join(" / ")} MHz` : "—",
          },
          { label: "连接数", value: s.conns ?? "—" },
          { label: "DHCP 租约", value: s.lan_leases },
        ]}
      />
    </>
  );
}

export function SystemTile({
  part,
}: {
  part: Part<SystemStatusInfo> | undefined;
}) {
  return (
    <Tile title="系统" icon={CpuIcon} to="/system/overview" part={part}>
      {(s) => <SystemBody s={s} />}
    </Tile>
  );
}
