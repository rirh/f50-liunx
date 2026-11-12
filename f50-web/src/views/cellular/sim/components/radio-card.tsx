import { Alert, AlertDescription } from "@/components/ui/alert";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import {
  Field,
  FieldContent,
  FieldDescription,
  FieldGroup,
  FieldLabel,
  FieldSeparator,
} from "@/components/ui/field";
import { Switch } from "@/components/ui/switch";
import { t } from "@/i18n";
import { reportFormError } from "@/lib/form-error";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { toast } from "sonner";
import { JobSlot } from "../../shared/job-slot";
import { useJobSlot } from "../../shared/use-job-slot";
import { setCardRadio } from "../api";
import type { CardSlot, SimStatus } from "../types";

/** Each card's radio setting, kept across reboots; both off is the persistent airplane mode. */
export function CardRadioCard({ sim }: { sim: SimStatus }) {
  const qc = useQueryClient();
  const { job, start } = useJobSlot();
  const refresh = () => qc.invalidateQueries({ queryKey: ["cellular"] });
  const card = useMutation({
    mutationFn: ({ slot, on }: { slot: CardSlot; on: boolean }) =>
      setCardRadio(slot, on),
    onSuccess: (r, { on }) => {
      if (r.job)
        start(
          r.job,
          on
            ? t("cellular.shared.jobRadioOn")
            : t("cellular.shared.jobRadioOff"),
        );
      else toast.success(t("cellular.sim.radio.savedNext"));
      refresh();
    },
    onError: (e) => reportFormError(e),
  });
  const disabled = sim.busy || card.isPending;
  const pendingSlot = card.isPending ? card.variables.slot : null;
  const rows: {
    slot: CardSlot;
    label: string;
    desc: string;
    checked: boolean;
  }[] = [
    {
      slot: "all",
      label: t("cellular.shared.airplane"),
      desc: t("cellular.sim.radio.airplaneHint"),
      checked: sim.radio.all_off,
    },
    ...([1, 2] as const).map((n) => ({
      slot: String(n) as CardSlot,
      label:
        sim.active === n
          ? t("cellular.sim.radio.cardLabelActive", { n })
          : t("cellular.sim.radio.cardLabel", { n }),
      desc:
        sim.active === n
          ? t("cellular.sim.radio.now")
          : t("cellular.sim.radio.next"),
      checked: sim.radio.enabled[n - 1],
    })),
  ];
  return (
    <Card>
      <CardHeader>
        <CardTitle>{t("cellular.sim.radio.title")}</CardTitle>
        <CardDescription>{t("cellular.sim.radio.description")}</CardDescription>
      </CardHeader>
      <CardContent className="flex flex-col gap-4">
        {sim.busy && (
          <Alert>
            <AlertDescription>{t("cellular.sim.radio.busy")}</AlertDescription>
          </Alert>
        )}
        <FieldGroup>
          {rows.map((r, i) => (
            <div key={r.slot} className="contents">
              {i === 1 && <FieldSeparator />}
              <Field orientation="horizontal" data-disabled={disabled}>
                <FieldContent>
                  <FieldLabel htmlFor={`sim-radio-${r.slot}`}>
                    {r.label}
                  </FieldLabel>
                  <FieldDescription>
                    {pendingSlot === r.slot
                      ? t("cellular.shared.saving")
                      : r.desc}
                  </FieldDescription>
                </FieldContent>
                <Switch
                  id={`sim-radio-${r.slot}`}
                  checked={r.checked}
                  disabled={disabled}
                  aria-busy={pendingSlot === r.slot}
                  onCheckedChange={(v) =>
                    card.mutate({ slot: r.slot, on: r.slot === "all" ? !v : v })
                  }
                />
              </Field>
            </div>
          ))}
        </FieldGroup>
        <JobSlot job={job} onDone={refresh} />
      </CardContent>
    </Card>
  );
}
