import { Link } from "react-router-dom";
import { QueryState } from "@/components/f50/query-state";
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import {
  Card,
  CardAction,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import { formatBytes, formatDuration } from "@/lib/format";
import { ArrowUpCircleIcon, FlameIcon } from "lucide-react";
import { LiveChart } from "./live-chart";
import { Meter } from "./meter";
import { useLiveStatus } from "../hooks/use-live-status";
import type { LiveStatus } from "../hooks/use-live-status";
import {
  formatCelsius,
  formatMhz,
  formatPercent,
  kb,
  memPercent,
  zoneLabel,
} from "../utils";

function usbText(s: LiveStatus["status"]["usb_link"]) {
  if (!s) return "未连接";
  const role = s.role === "device" ? "设备模式" : "主机模式";
  return `${role} · ${s.mbps != null ? `${s.mbps} Mbps` : (s.speed ?? "未知")}`;
}

function LiveBody({ data }: { data: LiveStatus }) {
  const { status: s, cpu, samples } = data;
  const mem = memPercent(s.memory);
  const used =
    s.memory.total_kb != null && s.memory.avail_kb != null
      ? s.memory.total_kb - s.memory.avail_kb
      : null;
  const t = s.temperatures;
  return (
    <div className="flex flex-col gap-4">
      {s.heat_alarm && (
        <Alert variant="destructive">
          <FlameIcon />
          <AlertTitle>过热告警</AlertTitle>
          <AlertDescription>
            设备温度过高，CPU 已被限频。请改善散热或降低负载。
          </AlertDescription>
        </Alert>
      )}
      {s.update && (
        <Alert>
          <ArrowUpCircleIcon />
          <AlertTitle>有新版本 {s.update}</AlertTitle>
          <AlertDescription className="flex flex-wrap items-center gap-2">
            更新服务发现了较新的系统版本。
            <Button asChild size="xs" variant="outline">
              <Link to="/system/update">前往更新</Link>
            </Button>
          </AlertDescription>
        </Alert>
      )}
      <div className="grid grid-cols-2 gap-3 sm:grid-cols-3 lg:grid-cols-4">
        <Meter
          label="CPU"
          value={formatPercent(cpu, 1)}
          percent={cpu}
          hint={
            s.load
              ? `${s.load.n} 核 · ${s.load.freqs.map((f) => formatMhz(f.cur)).join(" / ")}`
              : undefined
          }
        />
        <Meter
          label="内存"
          value={formatPercent(mem, 1)}
          percent={mem}
          hint={`${formatBytes(kb(used))} / ${formatBytes(kb(s.memory.total_kb))}`}
        />
        <Meter
          label="最高温度"
          value={formatCelsius(t.hottest)}
          hint={`SoC ${formatCelsius(t.soc)}`}
        />
        <Meter
          label="基带温度"
          value={formatCelsius(t.modem)}
          hint={`主板 ${formatCelsius(t.board)}`}
        />
        <Meter
          label="Wi-Fi 终端"
          value={s.wifi_clients}
          hint={`DHCP 租约 ${s.lan_leases}`}
        />
        <Meter
          label="连接跟踪"
          value={s.conns ?? "—"}
          hint="当前跟踪的连接数"
        />
        <Meter
          label="USB 链路"
          value={usbText(s.usb_link)}
          hint={s.usb_link?.max ? `最高 ${s.usb_link.max}` : undefined}
        />
        <Meter label="运行时间" value={formatDuration(s.uptime)} />
      </div>
      <LiveChart samples={samples} />
      {t.zones.length > 0 && (
        <div className="flex flex-wrap gap-2">
          {t.zones.map((z) => (
            <Badge key={z.name} variant="outline" className="font-mono">
              {zoneLabel(z.name)} {formatCelsius(z.millicelsius / 1000)}
            </Badge>
          ))}
        </div>
      )}
    </div>
  );
}

/** The live figures, polled every 3 s. */
export function LiveCard() {
  const query = useLiveStatus();
  return (
    <Card>
      <CardHeader>
        <CardTitle>实时状态</CardTitle>
        <CardDescription>
          每 3 秒刷新；CPU 占用为两次采样之间的平均值
        </CardDescription>
        <CardAction>
          {query.isFetching ? (
            <Badge variant="secondary">刷新中</Badge>
          ) : (
            <Badge variant="outline">实时</Badge>
          )}
        </CardAction>
      </CardHeader>
      <CardContent>
        <QueryState query={query} skeletonRows={6}>
          {(data) => <LiveBody data={data} />}
        </QueryState>
      </CardContent>
    </Card>
  );
}
