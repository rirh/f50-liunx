import { useState } from "react";
import { FormDialog } from "@/components/f50/form-dialog";
import { JobProgress } from "@/components/f50/job-progress";
import { KvList } from "@/components/f50/kv-list";
import { QueryState } from "@/components/f50/query-state";
import { Button } from "@/components/ui/button";
import {
  Card,
  CardAction,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import { Field, FieldError, FieldLabel } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { t } from "@/i18n";
import { errorMessage } from "@/lib/format";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { toast } from "sonner";
import { getIdentity } from "../../signal/api";
import { setImei } from "../api";
import type { SimStatus } from "../types";

const IDENTITY_KEY = ["modem", "identity"] as const;
const SLOT_LABEL = {
  1: "cellular.sim.slotName.s1",
  2: "cellular.sim.slotName.s2",
} as const;

function validImei(value: string) {
  if (value.length !== 15 || /[^0-9]/.test(value)) return false;

  let sum = 0;
  for (let i = 0; i < value.length; i++) {
    const digit = Number(value[i]);
    const weighted = i % 2 === 1 ? digit * 2 : digit;
    sum += weighted > 9 ? weighted - 9 : weighted;
  }
  return sum !== 0 && sum % 10 === 0;
}

export function ImeiCard({ sim }: { sim: SimStatus }) {
  const qc = useQueryClient();
  const [revealed, setRevealed] = useState(true);
  const [editSlot, setEditSlot] = useState<1 | 2 | null>(null);
  const [imei, setImeiInput] = useState("");
  const [job, setJob] = useState<{ id: string; running: boolean } | null>(null);
  const identity = useQuery({
    queryKey: IDENTITY_KEY,
    queryFn: ({ signal }) => getIdentity(signal),
    enabled: revealed,
    refetchInterval: revealed ? 10_000 : false,
    retry: false,
  });
  const save = useMutation({
    mutationFn: setImei,
    onSuccess: (r) => {
      void qc.invalidateQueries({ queryKey: IDENTITY_KEY, refetchType: "none" });
      setJob({ id: r.job, running: true });
      setEditSlot(null);
      setImeiInput("");
    },
    onError: (e) => toast.error(errorMessage(e)),
  });
  const slots = sim.slot2_supported ? ([1, 2] as const) : ([1] as const);
  const busy = sim.busy || save.isPending || job?.running;
  const valid = validImei(imei);
  const invalid = imei !== "" && !valid;
  const canWrite = editSlot === sim.active && valid && !busy;

  return (
    <Card className="lg:col-span-2">
      <CardHeader>
        <CardTitle>IMEI</CardTitle>
        <CardDescription>{t("cellular.sim.imei.description")}</CardDescription>
        <CardAction>
          <Button
            type="button"
            variant="outline"
            size="sm"
            aria-expanded={revealed}
            aria-controls="sim-imei-values"
            onClick={() => setRevealed(!revealed)}
          >
            {t(revealed ? "common.action.hide" : "common.action.show")}
          </Button>
        </CardAction>
      </CardHeader>
      <CardContent id="sim-imei-values" hidden={!revealed}>
        {revealed && (
          <QueryState query={identity} skeletonRows={slots.length}>
            {(d) => (
              <KvList
                items={slots.map((slot) => {
                  const value = d.imeis
                    ? d.imeis[slot - 1]
                    : d.slot === slot
                      ? d.ident?.imei
                      : null;
                  const original = d.original_imeis?.[slot - 1];
                  const active = slot === sim.active;
                  return {
                    label: t(SLOT_LABEL[slot]),
                    value: (
                      <div className="flex flex-col gap-1">
                        <div className="flex flex-wrap items-center gap-x-2 gap-y-1">
                          {value ? (
                            <span className="font-mono tabular-nums">{value}</span>
                          ) : (
                            <span className="text-muted-foreground">
                              {t("cellular.sim.imei.unknown")}
                            </span>
                          )}
                          {!active && value && (
                            <span className="text-xs text-muted-foreground">
                              {t("cellular.sim.imei.lastRead")}
                            </span>
                          )}
                          {active && (
                            <Button
                              type="button"
                              variant="outline"
                              size="sm"
                              disabled={busy}
                              aria-label={t("cellular.sim.imei.edit", {
                                slot: t(SLOT_LABEL[slot]),
                              })}
                              onClick={() => setEditSlot(slot)}
                            >
                              {t("common.action.edit")}
                            </Button>
                          )}
                        </div>
                        <div className="flex flex-wrap items-baseline gap-x-2 text-xs text-muted-foreground">
                          <span>{t("cellular.sim.imei.original")}</span>
                          {original ? (
                            <span className="font-mono tabular-nums">{original}</span>
                          ) : (
                            <span>{t("cellular.sim.imei.originalUnknown")}</span>
                          )}
                        </div>
                      </div>
                    ),
                  };
                })}
              />
            )}
          </QueryState>
        )}
      </CardContent>
      {job && (
        <CardContent>
          <JobProgress
            key={job.id}
            id={job.id}
            title={t("cellular.sim.imei.write")}
            onDone={(done) => {
              setJob({ id: done.id, running: false });
              void qc.invalidateQueries({ queryKey: IDENTITY_KEY });
            }}
          />
        </CardContent>
      )}
      {editSlot !== null && (
        <FormDialog
          open
          onOpenChange={(open) => {
            if (!open) {
              setEditSlot(null);
              setImeiInput("");
            }
          }}
          title={t("cellular.sim.imei.edit", { slot: t(SLOT_LABEL[editSlot]) })}
          description={t("cellular.sim.imei.confirm")}
          formId="sim-imei-form"
          size="sm"
          pending={save.isPending}
          dirty={canWrite}
          submitText={t("cellular.sim.imei.write")}
        >
          <form
            id="sim-imei-form"
            onSubmit={(event) => {
              event.preventDefault();
              if (canWrite) save.mutate({ slot: editSlot, imei });
            }}
          >
            <Field data-invalid={invalid}>
              <FieldLabel htmlFor="sim-imei-input">IMEI</FieldLabel>
              <Input
                id="sim-imei-input"
                inputMode="numeric"
                autoComplete="off"
                value={imei}
                aria-invalid={invalid}
                aria-describedby={invalid ? "sim-imei-error" : undefined}
                disabled={busy}
                onChange={(event) => setImeiInput(event.currentTarget.value)}
              />
              {invalid && (
                <FieldError id="sim-imei-error">
                  {t("cellular.sim.imei.invalid")}
                </FieldError>
              )}
            </Field>
          </form>
        </FormDialog>
      )}
    </Card>
  );
}
