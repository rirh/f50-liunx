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
import { type OnJob, useLockSet } from "../use-lock-set";

/** EN-DC (a job, no stack restart) and the boot replay switch (saved at once). */
export function OptionsCard({
  endc,
  autoApply,
  disabled,
  onJob,
}: {
  endc: boolean | null;
  autoApply: boolean;
  disabled: boolean;
  onJob: OnJob;
}) {
  const endcSet = useLockSet(onJob);
  const replaySet = useLockSet(onJob);
  return (
    <Card>
      <CardHeader>
        <CardTitle>{t("cellular.locks.options.title")}</CardTitle>
        <CardDescription>
          {t("cellular.locks.options.description")}
        </CardDescription>
      </CardHeader>
      <CardContent>
        <FieldGroup>
          <Field
            orientation="horizontal"
            data-disabled={disabled || endcSet.isPending}
          >
            <FieldContent>
              <FieldLabel htmlFor="lock-endc">
                {t("cellular.locks.options.endc")}
              </FieldLabel>
              <FieldDescription>
                {endcSet.isPending
                  ? t("cellular.locks.options.submitting")
                  : endc == null
                    ? t("cellular.locks.options.endcUnknown")
                    : t("cellular.locks.options.endcHint")}
              </FieldDescription>
            </FieldContent>
            <Switch
              id="lock-endc"
              checked={endc ?? false}
              disabled={disabled || endcSet.isPending}
              aria-busy={endcSet.isPending}
              onCheckedChange={(on) =>
                endcSet.mutate(
                  {
                    kind: "endc",
                    value: on ? "on" : "off",
                    title: on
                      ? t("cellular.locks.options.endcOn")
                      : t("cellular.locks.options.endcOff"),
                  },
                  { onError: (e) => reportFormError(e) },
                )
              }
            />
          </Field>
          <FieldSeparator />
          <Field orientation="horizontal" data-disabled={replaySet.isPending}>
            <FieldContent>
              <FieldLabel htmlFor="lock-auto-apply">
                {t("cellular.locks.options.autoApply")}
              </FieldLabel>
              <FieldDescription>
                {replaySet.isPending
                  ? t("cellular.shared.saving")
                  : t("cellular.locks.options.autoApplyHint")}
              </FieldDescription>
            </FieldContent>
            <Switch
              id="lock-auto-apply"
              checked={autoApply}
              disabled={replaySet.isPending}
              aria-busy={replaySet.isPending}
              onCheckedChange={(on) =>
                replaySet.mutate(
                  { kind: "auto_apply", value: on ? "on" : "off", title: "" },
                  { onError: (e) => reportFormError(e) },
                )
              }
            />
          </Field>
        </FieldGroup>
      </CardContent>
    </Card>
  );
}
