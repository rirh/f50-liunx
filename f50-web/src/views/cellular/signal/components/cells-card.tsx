import type { ReactNode } from "react";
import { Row } from "@/components/f50/grouped-list";
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
import { usePoll } from "@/hooks/use-live";
import { t } from "@/i18n";
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

const sigSrcLabel = (src: "nr" | "lte" | "cesq") =>
  src === "nr"
    ? t("cellular.signal.cells.nrServing")
    : src === "lte"
      ? t("cellular.signal.cells.lteServing")
      : "CESQ";

function lteItems(c: LteServing): Fact[] {
  return [
    {
      label: t("cellular.signal.cells.bandEarfcn"),
      value: `B${c.band} / ${c.earfcn}`,
    },
    { label: "PCI", value: c.pci },
    { label: t("cellular.signal.cells.bandwidth"), value: c.bw || "—" },
    { label: "eNB / CID", value: `${c.enb ?? "—"} / ${c.cid ?? "—"}` },
    { label: "TAC", value: c.tac ? c.tac.toUpperCase() : "—" },
    {
      label: "RSRP / RSRQ",
      value: `${unit(c.rsrp, "dBm")} / ${unit(c.rsrq, "dB")}`,
    },
    { label: "SINR", value: unit(c.sinr, "dB") },
    {
      label: t("cellular.signal.cells.mcs"),
      value: `${c.dl_mcs ?? "—"} / ${c.ul_mcs ?? "—"}`,
    },
    {
      label: t("cellular.signal.cells.bler"),
      value: `${unit(c.dl_bler, "%")} / ${unit(c.ul_bler, "%")}`,
    },
  ];
}

function nrItems(c: NrServing): Fact[] {
  return [
    {
      label: t("cellular.signal.cells.bandArfcn"),
      value: `n${c.band} / ${c.arfcn}`,
    },
    { label: "PCI", value: c.pci },
    {
      label: t("cellular.signal.cells.bandwidth"),
      value: unit(c.bw_mhz, "MHz"),
    },
    { label: "gNB / CID", value: `${c.gnb ?? "—"} / ${c.cid ?? "—"}` },
    {
      label: "RSRP / RSRQ",
      value: `${unit(c.rsrp, "dBm")} / ${unit(c.rsrq, "dB")}`,
    },
    { label: "SINR", value: unit(c.sinr, "dB") },
    {
      label: t("cellular.signal.cells.mcs"),
      value: `${c.dl_mcs ?? "—"} / ${c.ul_mcs ?? "—"}`,
    },
    {
      label: t("cellular.signal.cells.bler"),
      value: `${unit(c.dl_bler, "%")} / ${unit(c.ul_bler, "%")}`,
    },
  ];
}

function regText(r: Registration | null | undefined) {
  if (!r || r.stat == null) return "—";
  const tail = [r.tac && `TAC ${r.tac}`, r.cid && `CID ${r.cid}`]
    .filter(Boolean)
    .join(" · ");
  const k = REG_STAT_LABEL[r.stat];
  return `${k ? t(k) : r.stat}${tail ? ` · ${tail}` : ""}`;
}

function qosText(q: Qos | null | undefined) {
  if (!q) return "—";
  return t("cellular.signal.cells.qos", {
    qci: q.qci,
    dl: unit(q.dl, "Mbit/s"),
    ul: unit(q.ul, "Mbit/s"),
  });
}

interface Fact {
  label: string;
  value: ReactNode;
}

/** A settings-style group: every value whole (wrapping, never cut). */
function Facts({ title, items }: { title?: ReactNode; items: Fact[] }) {
  return (
    <section className="flex min-w-0 flex-col gap-1.5">
      {title && (
        <h3 className="flex items-center gap-2 px-3 text-xs font-medium text-muted-foreground">
          {title}
        </h3>
      )}
      <div className="divide-y divide-border/60 overflow-hidden rounded-xl bg-muted/40">
        {items.map((f) => (
          <Row key={f.label} label={f.label} wrap>
            {f.value}
          </Row>
        ))}
      </div>
    </section>
  );
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
    <section className="flex flex-col gap-2">
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
        <AlertTitle>{t("cellular.signal.cells.noReading")}</AlertTitle>
        <AlertDescription>{d.error}</AlertDescription>
      </Alert>
    ) : (
      <Empty>
        <EmptyHeader>
          <EmptyTitle>{t("cellular.signal.cells.emptyTitle")}</EmptyTitle>
          <EmptyDescription>
            {t("cellular.signal.cells.emptyDescription")}
          </EmptyDescription>
        </EmptyHeader>
      </Empty>
    );
  }
  const ca = d.lte?.ca === "DL_CA";
  return (
    <div className="flex flex-col gap-5">
      <div className="grid items-start gap-4 lg:grid-cols-2 2xl:grid-cols-3">
        <Facts
          title={t("cellular.signal.cells.network")}
          items={[
            { label: t("cellular.signal.cells.regLte"), value: regText(d.reg) },
            {
              label: t("cellular.signal.cells.reg5g"),
              value: regText(d.reg5g),
            },
            {
              label: t("cellular.shared.operator"),
              value: d.operator
                ? [d.operator.name, d.operator.plmn]
                    .filter(Boolean)
                    .join(" · ") || "—"
                : "—",
            },
            {
              label: t("cellular.signal.cells.sigSrc"),
              value: d.sig_src ? sigSrcLabel(d.sig_src) : "—",
            },
            {
              label: t("cellular.signal.cells.ca"),
              value: ca
                ? t("cellular.signal.cells.caOn", { bw: d.lte?.bw })
                : t("cellular.signal.cells.caOff"),
            },
            {
              label: t("cellular.signal.cells.bearerQos"),
              value: qosText(d.qos),
            },
          ]}
        />
        {d.nr && (
          <Facts
            title={t("cellular.signal.cells.nrServing")}
            items={nrItems(d.nr)}
          />
        )}
        {d.lte && (
          <Facts
            title={
              <>
                {t("cellular.signal.cells.lteServing")}
                {ca && <Badge variant="secondary">CA</Badge>}
              </>
            }
            items={lteItems(d.lte)}
          />
        )}
      </div>
      {!d.nr && !d.lte && (
        <p className="text-sm text-muted-foreground">
          {t("cellular.signal.cells.noLte")} · {t("cellular.signal.cells.noNr")}
        </p>
      )}
      <Section
        title={t("cellular.signal.cells.neighbours")}
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
    refetchInterval: usePoll(5000, "cells"),
  });
  return (
    <Card>
      <CardHeader>
        <CardTitle>{t("cellular.signal.cells.title")}</CardTitle>
        <CardDescription>
          {t("cellular.signal.cells.description", {
            age: formatAgeMs(query.data?.age_ms),
          })}
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
