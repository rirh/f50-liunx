import { ConfirmButton } from "@/components/f50/confirm-button";
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
} from "@/components/ui/field";
import { Switch } from "@/components/ui/switch";
import { usePoll } from "@/hooks/use-live";
import { t } from "@/i18n";
import { reportFormError } from "@/lib/form-error";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { toast } from "sonner";
import { JobSlot } from "../../shared/job-slot";
import { useJobSlot } from "../../shared/use-job-slot";
import { getStatus, restartModem, setRadio, signalKeys } from "../api";

/** The card in use's radio (kept across reboots) and the modem's protocol-stack restart. */
export function RadioCard() {
  const qc = useQueryClient();
  const status = useQuery({
    queryKey: signalKeys.status,
    queryFn: getStatus,
    refetchInterval: usePoll(2000, "modem"),
  });
  const { job, start } = useJobSlot();
  const refresh = () => qc.invalidateQueries({ queryKey: ["cellular"] });

  const radio = useMutation({
    mutationFn: setRadio,
    onSuccess: (r, on) => {
      if (r.job)
        start(
          r.job,
          on
            ? t("cellular.shared.jobRadioOn")
            : t("cellular.shared.jobRadioOff"),
        );
      else toast.success(t("cellular.signal.radio.savedNext"));
      refresh();
    },
    onError: (e) => reportFormError(e),
  });
  const restart = useMutation({
    mutationFn: restartModem,
    onSuccess: (r) => start(r.job, t("cellular.signal.radio.jobRestart")),
    onError: (e) => reportFormError(e),
  });

  const s = status.data;
  const busy = !s || s.restarting || radio.isPending || restart.isPending;
  return (
    <Card>
      <CardHeader>
        <CardTitle>{t("cellular.signal.radio.title")}</CardTitle>
        <CardDescription>
          {t("cellular.signal.radio.description")}
        </CardDescription>
      </CardHeader>
      <CardContent className="flex flex-col gap-4">
        <FieldGroup>
          <Field orientation="horizontal" data-disabled={busy}>
            <FieldContent>
              <FieldLabel htmlFor="modem-radio">
                {s
                  ? t("cellular.signal.radio.labelSlot", { slot: s.slot })
                  : t("cellular.signal.radio.label")}
              </FieldLabel>
              <FieldDescription>
                {radio.isPending
                  ? t("cellular.signal.radio.switching")
                  : t("cellular.signal.radio.offHint")}
              </FieldDescription>
            </FieldContent>
            <Switch
              id="modem-radio"
              checked={s?.radio_allowed ?? false}
              disabled={busy}
              aria-busy={radio.isPending}
              onCheckedChange={(on) => radio.mutate(on)}
            />
          </Field>
          <Field orientation="horizontal">
            <FieldContent>
              <FieldLabel>{t("cellular.signal.radio.restart")}</FieldLabel>
              <FieldDescription>
                {t("cellular.signal.radio.restartHint")}
              </FieldDescription>
            </FieldContent>
            <ConfirmButton
              size="sm"
              title={t("cellular.signal.radio.confirmTitle")}
              description={t("cellular.signal.radio.confirmDescription")}
              confirmText={t("common.action.reboot")}
              pending={restart.isPending}
              disabled={busy || !s?.radio_allowed}
              onConfirm={() => restart.mutate()}
            >
              {t("common.action.reboot")}
            </ConfirmButton>
          </Field>
        </FieldGroup>
        <JobSlot job={job} onDone={refresh} />
      </CardContent>
    </Card>
  );
}
