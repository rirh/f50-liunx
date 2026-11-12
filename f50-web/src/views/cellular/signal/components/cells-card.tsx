import type { ReactNode } from "react";
import { KvList, type KvItem } from "@/components/f50/kv-list";
import { QueryState } from "@/components/f50/query-state";
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert";
import { Badge } from "@/components/ui/badge";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import {
  Empty,
  EmptyDescription,
  EmptyHeader,
  EmptyTitle,
} from "@/components/ui/empty";
import { Separator } from "@/components/ui/separator";
import { useQuery } from "@tanstack/react-query";
import { NeighbourTable } from "./neighbour-table";
import { formatAgeMs, unit } from "../../shared/format";
import { getCells, signalKeys } from "../api";
import { REG_STAT_LABEL } from "../labels";
import type {
  CellDetail,
  LteServing,
  NrServing,
  Qos,
  Registration,
} from "../types";

const SIG_SRC_LABEL = {
  nr: "NR 服务小区",
  lte: "LTE 服务小区",
  cesq: "CESQ",
} as const;

function lteItems(c: LteServing): KvItem[] {
  return [
    { label: "频段 / EARFCN", value: `B${c.band} / ${c.earfcn}` },
    { label: "PCI", value: c.pci },
    { label: "带宽", value: c.bw || "—" },
    { label: "eNB / CID", value: `${c.enb ?? "—"} / ${c.cid ?? "—"}` },
    { label: "TAC", value: c.tac ? c.tac.toUpperCase() : "—" },
    {
      label: "RSRP / RSRQ",
      value: `${unit(c.rsrp, "dBm")} / ${unit(c.rsrq, "dB")}`,
    },
    { label: "SINR", value: unit(c.sinr, "dB") },
    { label: "MCS 下/上", value: `${c.dl_mcs ?? "—"} / ${c.ul_mcs ?? "—"}` },
    {
      label: "BLER 下/上",
      value: `${unit(c.dl_bler, "%")} / ${unit(c.ul_bler, "%")}`,
    },
  ];
}

function nrItems(c: NrServing): KvItem[] {
  return [
    { label: "频段 / ARFCN", value: `n${c.band} / ${c.arfcn}` },
    { label: "PCI", value: c.pci },
    { label: "带宽", value: unit(c.bw_mhz, "MHz") },
    { label: "gNB / CID", value: `${c.gnb ?? "—"} / ${c.cid ?? "—"}` },
    {
      label: "RSRP / RSRQ",
      value: `${unit(c.rsrp, "dBm")} / ${unit(c.rsrq, "dB")}`,
    },
    { label: "SINR", value: unit(c.sinr, "dB") },
    { label: "MCS 下/上", value: `${c.dl_mcs ?? "—"} / ${c.ul_mcs ?? "—"}` },
    {
      label: "BLER 下/上",
      value: `${unit(c.dl_bler, "%")} / ${unit(c.ul_bler, "%")}`,
    },
  ];
}

function regText(r: Registration | null | undefined) {
  if (!r || r.stat == null) return "—";
  const tail = [r.tac && `TAC ${r.tac}`, r.cid && `CID ${r.cid}`]
    .filter(Boolean)
    .join(" · ");
  return `${REG_STAT_LABEL[r.stat] ?? r.stat}${tail ? ` · ${tail}` : ""}`;
}

function qosText(q: Qos | null | undefined) {
  if (!q) return "—";
  return `QCI ${q.qci} · 下行 ${unit(q.dl, "Mbit/s")} · 上行 ${unit(q.ul, "Mbit/s")}`;
}

function Section({
  title,
  badge,
  children,
}: {
  title: string;
  badge?: ReactNode;
  children: ReactNode;
}) {
  return (
    <section className="flex flex-col gap-3">
      <h3 className="flex items-center gap-2 text-sm font-medium">
        {title}
        {badge}
      </h3>
      {children}
    </section>
  );
}

function CellsBody({ d }: { d: CellDetail | null }) {
  if (!d || d.error) {
    return d?.error ? (
      <Alert variant="destructive">
        <AlertTitle>没有小区读数</AlertTitle>
        <AlertDescription>{d.error}</AlertDescription>
      </Alert>
    ) : (
      <Empty>
        <EmptyHeader>
          <EmptyTitle>尚无读数</EmptyTitle>
          <EmptyDescription>采集器还没有完成第一轮读取。</EmptyDescription>
        </EmptyHeader>
      </Empty>
    );
  }
  const ca = d.lte?.ca === "DL_CA";
  return (
    <div className="flex flex-col gap-5">
      <KvList
        items={[
          { label: "LTE 注册", value: regText(d.reg) },
          { label: "5G 注册", value: regText(d.reg5g) },
          {
            label: "运营商",
            value: d.operator
              ? [d.operator.name, d.operator.plmn]
                  .filter(Boolean)
                  .join(" · ") || "—"
              : "—",
          },
          {
            label: "信号来源",
            value: d.sig_src ? SIG_SRC_LABEL[d.sig_src] : "—",
          },
          {
            label: "载波聚合",
            value: ca ? `下行 CA（${d.lte?.bw}）` : "未启用",
          },
          { label: "承载 QoS", value: qosText(d.qos) },
        ]}
      />
      <div className="grid gap-5 lg:grid-cols-2">
        <Section
          title="LTE 服务小区"
          badge={ca && <Badge variant="secondary">CA</Badge>}
        >
          {d.lte ? (
            <KvList items={lteItems(d.lte)} />
          ) : (
            <p className="text-sm text-muted-foreground">无 LTE 服务小区</p>
          )}
        </Section>
        <Section title="NR 服务小区">
          {d.nr ? (
            <KvList items={nrItems(d.nr)} />
          ) : (
            <p className="text-sm text-muted-foreground">无 NR 服务小区</p>
          )}
        </Section>
      </div>
      <Separator />
      <Section
        title="邻区"
        badge={<Badge variant="outline">{d.neigh?.length ?? 0}</Badge>}
      >
        <NeighbourTable rows={d.neigh ?? []} />
      </Section>
    </div>
  );
}

export function CellsCard() {
  const query = useQuery({
    queryKey: signalKeys.cells,
    queryFn: getCells,
    refetchInterval: 5000,
  });
  return (
    <Card>
      <CardHeader>
        <CardTitle>小区信息</CardTitle>
        <CardDescription>
          服务小区、邻区、载波聚合与 QoS · 每 5 秒刷新 · 读数{" "}
          {formatAgeMs(query.data?.age_ms)}
        </CardDescription>
      </CardHeader>
      <CardContent>
        <QueryState query={query} skeletonRows={8}>
          {(c) => <CellsBody d={c.detail} />}
        </QueryState>
      </CardContent>
    </Card>
  );
}
