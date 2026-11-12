import { useState } from "react";
import { KvList } from "@/components/f50/kv-list";
import { PendingButton } from "@/components/f50/pending-button";
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert";
import { Badge } from "@/components/ui/badge";
import {
  Card,
  CardContent,
  CardDescription,
  CardFooter,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import { Field, FieldDescription, FieldLabel } from "@/components/ui/field";
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group";
import { t, type Key } from "@/i18n";
import { reportFormError } from "@/lib/form-error";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { JobSlot } from "../../shared/job-slot";
import { useJobSlot } from "../../shared/use-job-slot";
import { selectSlot, simKeys } from "../api";
import type { SimState, SimStatus } from "../types";

const STATE_LABEL: Record<SimState, Key> = {
  ready: "cellular.sim.state.ready",
  absent: "cellular.sim.state.absent",
  locked: "cellular.sim.state.locked",
};
const SLOT_LABEL: Record<1 | 2, Key> = {
  1: "cellular.sim.slotName.s1",
  2: "cellular.sim.slotName.s2",
};

/** The slot in use and the one the next boot takes; a selection only applies after a reboot. */
export function SlotCard({ sim }: { sim: SimStatus }) {
  const qc = useQueryClient();
  const [want, setWant] = useState<string>(String(sim.default));
  const { job, start } = useJobSlot();
  const select = useMutation({
    mutationFn: selectSlot,
    onSuccess: (r) => start(r.job, t("cellular.sim.slot.job")),
    onError: (e) => reportFormError(e),
  });
  const target = Number(want) as 1 | 2;
  return (
    <Card>
      <CardHeader>
        <CardTitle>{t("cellular.sim.slot.title")}</CardTitle>
        <CardDescription>{t("cellular.sim.slot.description")}</CardDescription>
      </CardHeader>
      <CardContent className="flex flex-col gap-4">
        <KvList
          items={[
            {
              label: t("cellular.sim.slot.active"),
              value: t(SLOT_LABEL[sim.active]),
            },
            {
              label: t("cellular.sim.slot.cardState"),
              value: sim.state ? (
                <Badge
                  variant={sim.state === "ready" ? "secondary" : "destructive"}
                >
                  {t(STATE_LABEL[sim.state])}
                </Badge>
              ) : (
                t("cellular.shared.notDetected")
              ),
            },
            {
              label: t("cellular.sim.slot.nextBoot"),
              value: t(SLOT_LABEL[sim.default]),
            },
          ]}
        />
        {sim.reboot_required && (
          <Alert>
            <AlertTitle>{t("cellular.sim.slot.pendingTitle")}</AlertTitle>
            <AlertDescription>
              {t("cellular.sim.slot.pendingDescription", {
                next: t(SLOT_LABEL[sim.default]),
                active: t(SLOT_LABEL[sim.active]),
              })}
            </AlertDescription>
          </Alert>
        )}
        <Field>
          <FieldLabel>{t("cellular.sim.slot.choose")}</FieldLabel>
          <ToggleGroup
            type="single"
            variant="outline"
            value={want}
            onValueChange={(v) => v && setWant(v)}
            disabled={select.isPending}
          >
            <ToggleGroupItem value="1">
              {t("cellular.sim.slot.external")}
            </ToggleGroupItem>
            <ToggleGroupItem value="2" disabled={!sim.slot2_supported}>
              {t("cellular.sim.slot.internal")}
            </ToggleGroupItem>
          </ToggleGroup>
          <FieldDescription>{t("cellular.sim.slot.hint")}</FieldDescription>
        </Field>
        <JobSlot
          job={job}
          onDone={() => qc.invalidateQueries({ queryKey: simKeys.get })}
        />
      </CardContent>
      <CardFooter>
        <PendingButton
          pending={select.isPending}
          disabled={target === sim.default || sim.busy}
          onClick={() => select.mutate(target)}
        >
          {t("common.action.save")}
        </PendingButton>
      </CardFooter>
    </Card>
  );
}
