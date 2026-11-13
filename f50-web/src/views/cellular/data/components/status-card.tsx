import { ConfirmButton } from "@/components/f50/confirm-button";
import { KvList } from "@/components/f50/kv-list";
import { PendingButton } from "@/components/f50/pending-button";
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert";
import { Badge } from "@/components/ui/badge";
import {
  Card,
  CardAction,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import { t, type Key } from "@/i18n";
import { reportFormError } from "@/lib/form-error";
import { formatDuration } from "@/lib/format";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { toast } from "sonner";
import { dataKeys, setDataPower } from "../api";
import type { DataHold, DataPhase, DataStatus } from "../types";

const PHASE: Record<
  DataPhase,
  {
    label: Key;
    variant: "default" | "secondary" | "destructive" | "outline";
  }
> = {
  connected: { label: "cellular.data.phase.connected", variant: "default" },
  connecting: { label: "cellular.data.phase.connecting", variant: "outline" },
  held: { label: "cellular.data.phase.held", variant: "secondary" },
  failed: { label: "cellular.data.phase.failed", variant: "destructive" },
  down: { label: "cellular.data.phase.down", variant: "secondary" },
};

const HOLD: Record<DataHold, Key> = {
  roaming: "cellular.data.hold.roaming",
  "sim-missing": "cellular.data.hold.sim-missing",
  "radio-off": "cellular.data.hold.radio-off",
};

export function StatusCard({ d }: { d: DataStatus }) {
  const qc = useQueryClient();
  const power = useMutation({
    mutationFn: setDataPower,
    onSuccess: (r) => {
      toast.success(
        r.state === "on"
          ? t("cellular.data.status.turnedOn")
          : t("cellular.data.status.turnedOff"),
      );
      qc.invalidateQueries({ queryKey: dataKeys.get });
    },
    onError: (e) => reportFormError(e),
  });
  const on = d.state !== "down";
  const phase = PHASE[d.state];
  return (
    <Card>
      <CardHeader>
        <CardTitle>{t("cellular.data.status.title")}</CardTitle>
        <CardDescription>
          {t("cellular.data.status.description", {
            iface: d.interface,
            slot: d.slot,
          })}
        </CardDescription>
        <CardAction>
          <Badge variant={phase.variant}>{t(phase.label)}</Badge>
        </CardAction>
      </CardHeader>
      <CardContent className="flex flex-col gap-4">
        {d.hold && (
          <Alert>
            <AlertTitle>{t("cellular.data.status.heldTitle")}</AlertTitle>
            <AlertDescription>{t(HOLD[d.hold])}</AlertDescription>
          </Alert>
        )}
        {d.state === "failed" && d.error && (
          <Alert variant="destructive">
            <AlertTitle>{t("cellular.data.status.dialFailed")}</AlertTitle>
            <AlertDescription>{d.error}</AlertDescription>
          </Alert>
        )}
        <KvList
          items={[
            {
              label: t("cellular.data.status.duration"),
              value: formatDuration(d.since_s),
            },
            {
              label: t("cellular.data.status.device"),
              value: <span className="tabular-nums">{d.device}</span>,
            },
            {
              label: t("cellular.data.status.addresses"),
              value: d.addresses.length ? (
                <span className="flex flex-col tabular-nums">
                  {d.addresses.map((a) => (
                    <span key={a} className="break-all">
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
                <span className="tabular-nums break-all">
                  {d.dns.join(", ")}
                </span>
              ) : (
                "—"
              ),
            },
            {
              label: t("cellular.data.status.relay"),
              value: d.relay ? t("common.state.yes") : t("common.state.no"),
            },
            {
              label: t("cellular.data.status.iccidBound"),
              value: d.iccid_bound
                ? t("common.state.yes")
                : t("common.state.no"),
            },
          ]}
        />
        <div className="flex flex-wrap gap-2">
          {on ? (
            <ConfirmButton
              title={t("cellular.data.status.confirmTitle")}
              description={t("cellular.data.status.confirmDescription")}
              confirmText={t("common.action.close")}
              pending={power.isPending}
              onConfirm={() => power.mutate(false)}
            >
              {t("cellular.data.status.turnOff")}
            </ConfirmButton>
          ) : (
            <PendingButton
              pending={power.isPending}
              onClick={() => power.mutate(true)}
            >
              {t("cellular.data.status.turnOn")}
            </PendingButton>
          )}
        </div>
      </CardContent>
    </Card>
  );
}
