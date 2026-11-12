import { useState } from "react";
import { PendingButton } from "@/components/f50/pending-button";
import { Badge } from "@/components/ui/badge";
import {
  Card,
  CardAction,
  CardContent,
  CardDescription,
  CardFooter,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import { Field, FieldDescription, FieldLabel } from "@/components/ui/field";
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group";
import { t } from "@/i18n";
import { reportFormError } from "@/lib/form-error";
import type { ModeLabel } from "../types";
import { type OnJob, useLockSet } from "../use-lock-set";
import { MODES } from "../values";

export function ModeCard({
  current,
  disabled,
  onJob,
}: {
  current: ModeLabel;
  disabled: boolean;
  onJob: OnJob;
}) {
  const [mode, setMode] = useState<ModeLabel>(current);
  const set = useLockSet(onJob);
  const label = (m: ModeLabel) => {
    const k = MODES.find((x) => x.value === m)?.label;
    return k ? t(k) : m;
  };
  const hint = MODES.find((m) => m.value === mode)?.hint;
  return (
    <Card>
      <CardHeader>
        <CardTitle>{t("cellular.locks.mode.title")}</CardTitle>
        <CardDescription>
          {t("cellular.locks.mode.description")}
        </CardDescription>
        <CardAction>
          <Badge variant="secondary">
            {t("cellular.locks.current", { value: label(current) })}
          </Badge>
        </CardAction>
      </CardHeader>
      <CardContent>
        <Field>
          <FieldLabel>{t("cellular.locks.mode.label")}</FieldLabel>
          <ToggleGroup
            type="single"
            variant="outline"
            className="flex-wrap"
            value={mode}
            disabled={disabled || set.isPending}
            onValueChange={(v) => v && setMode(v as ModeLabel)}
          >
            {MODES.map((m) => (
              <ToggleGroupItem key={m.value} value={m.value}>
                {t(m.label)}
              </ToggleGroupItem>
            ))}
          </ToggleGroup>
          <FieldDescription>{hint && t(hint)}</FieldDescription>
        </Field>
      </CardContent>
      <CardFooter>
        <PendingButton
          pending={set.isPending}
          disabled={disabled || mode === current}
          onClick={() =>
            set.mutate(
              {
                kind: "mode",
                value: mode,
                title: t("cellular.locks.mode.job", { mode: label(mode) }),
              },
              { onError: (e) => reportFormError(e) },
            )
          }
        >
          {t("common.action.apply")}
        </PendingButton>
      </CardFooter>
    </Card>
  );
}
