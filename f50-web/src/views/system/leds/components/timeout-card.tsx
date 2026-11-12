import { useMemo } from "react";
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
import {
  Field,
  FieldDescription,
  FieldError,
  FieldGroup,
  FieldLabel,
} from "@/components/ui/field";
import {
  InputGroup,
  InputGroupAddon,
  InputGroupInput,
  InputGroupText,
} from "@/components/ui/input-group";
import { t } from "@/i18n";
import { reportFormError } from "@/lib/form-error";
import { formatDuration } from "@/lib/format";
import { zodResolver } from "@hookform/resolvers/zod";
import { Controller, useForm } from "react-hook-form";
import { useLedSet } from "./use-led-set";
import { type TimeoutValues, timeoutSchema } from "../schemas";
import type { LedSettings } from "../types";

const describe = (s: number) =>
  s === 0
    ? t("system.leds.timeout.always")
    : t("system.leds.timeout.after", { duration: formatDuration(s) });

export function TimeoutCard({ data }: { data: LedSettings }) {
  const schema = useMemo(() => timeoutSchema(), []);
  const form = useForm<TimeoutValues>({
    resolver: zodResolver(schema),
    values: { seconds: String(data.timeout) },
  });
  const onError = (e: unknown) =>
    reportFormError(e, form.setError, { value: "seconds" });
  const save = useLedSet("timeout", onError);
  const reset = useLedSet("timeout", onError);
  const busy = save.isPending || reset.isPending;
  return (
    <Card>
      <CardHeader>
        <CardTitle>{t("system.leds.timeout.title")}</CardTitle>
        <CardDescription>
          {t("system.leds.timeout.current", { text: describe(data.timeout) })}
        </CardDescription>
        <CardAction>
          {data.timeout_default ? (
            <Badge variant="secondary">
              {t("system.leds.timeout.default")}
            </Badge>
          ) : (
            <Badge variant="outline">{t("system.leds.timeout.custom")}</Badge>
          )}
        </CardAction>
      </CardHeader>
      <form
        onSubmit={form.handleSubmit((v) => save.mutate(v.seconds))}
        className="contents"
      >
        <CardContent>
          <FieldGroup>
            <Controller
              name="seconds"
              control={form.control}
              render={({ field, fieldState }) => (
                <Field data-invalid={fieldState.invalid}>
                  <FieldLabel htmlFor="led-timeout">
                    {t("system.leds.timeout.label")}
                  </FieldLabel>
                  <InputGroup>
                    <InputGroupInput
                      {...field}
                      id="led-timeout"
                      inputMode="numeric"
                      autoComplete="off"
                      aria-invalid={fieldState.invalid}
                    />
                    <InputGroupAddon align="inline-end">
                      <InputGroupText>
                        {t("system.leds.timeout.seconds")}
                      </InputGroupText>
                    </InputGroupAddon>
                  </InputGroup>
                  <FieldDescription>
                    {t("system.leds.timeout.hint", {
                      n: data.default_timeout,
                      text: describe(data.default_timeout),
                    })}
                  </FieldDescription>
                  {fieldState.invalid && (
                    <FieldError errors={[fieldState.error]} />
                  )}
                </Field>
              )}
            />
          </FieldGroup>
        </CardContent>
        <CardFooter className="flex flex-wrap gap-2">
          <PendingButton
            type="submit"
            pending={save.isPending}
            disabled={busy || !form.formState.isDirty}
          >
            {t("common.action.save")}
          </PendingButton>
          <PendingButton
            type="button"
            variant="outline"
            pending={reset.isPending}
            disabled={busy || data.timeout_default}
            onClick={() => reset.mutate("default")}
          >
            {t("common.action.restore")}
          </PendingButton>
        </CardFooter>
      </form>
    </Card>
  );
}
