import { useMemo } from "react";
import { ConfirmButton } from "@/components/f50/confirm-button";
import { PendingButton } from "@/components/f50/pending-button";
import { Button } from "@/components/ui/button";
import {
  Card,
  CardContent,
  CardDescription,
  CardFooter,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import {
  Field,
  FieldContent,
  FieldDescription,
  FieldError,
  FieldGroup,
  FieldLabel,
  FieldSeparator,
} from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import {
  InputGroup,
  InputGroupAddon,
  InputGroupInput,
  InputGroupText,
} from "@/components/ui/input-group";
import { Switch } from "@/components/ui/switch";
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group";
import { t } from "@/i18n";
import { errorMessage } from "@/lib/format";
import { ApiError } from "@/lib/ubus";
import { zodResolver } from "@hookform/resolvers/zod";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { Controller, useForm } from "react-hook-form";
import { toast } from "sonner";
import { clearTraffic, setTraffic, trafficKeys } from "../api";
import {
  type CapUnit,
  type TrafficForm,
  UNITS,
  capBytes,
  capDisplay,
  trafficSchema,
} from "../schema";
import type { TrafficKey, TrafficStatus } from "../types";

function defaults(t: TrafficStatus): TrafficForm {
  return {
    reset_day: String(t.reset_day),
    ...capDisplay(t.cap.bytes),
    warn: String(t.cap.warn),
    cut: t.cap.cut,
    plan: t.plan,
  };
}

/** A failed `set` of one key, so its refusal lands on that field. */
class KeyError extends Error {
  readonly key: Exclude<TrafficKey, "clear">;
  readonly inner: unknown;

  constructor(key: Exclude<TrafficKey, "clear">, inner: unknown) {
    super(errorMessage(inner));
    this.key = key;
    this.inner = inner;
  }
}

type Change = [Exclude<TrafficKey, "clear">, string];

export function SettingsCard({ t: tr }: { t: TrafficStatus }) {
  const qc = useQueryClient();
  const form = useForm<TrafficForm>({
    resolver: zodResolver(useMemo(() => trafficSchema(), [])),
    defaultValues: defaults(tr),
  });
  const refresh = () => qc.invalidateQueries({ queryKey: trafficKeys.get });

  const save = useMutation({
    // one key per call: sent in order, the first refusal stops the rest
    mutationFn: async (changes: Change[]) => {
      for (const [key, value] of changes) {
        try {
          await setTraffic(key, value);
        } catch (e) {
          throw new KeyError(key, e);
        }
      }
    },
    onSuccess: () => toast.success(t("cellular.shared.saved")),
    onError: (e) => {
      if (
        e instanceof KeyError &&
        e.inner instanceof ApiError &&
        e.inner.field === "value"
      ) {
        form.setError(
          e.key,
          { type: "server", message: e.message },
          { shouldFocus: true },
        );
      } else toast.error(errorMessage(e instanceof KeyError ? e.inner : e));
    },
    onSettled: refresh,
  });
  const clear = useMutation({
    mutationFn: clearTraffic,
    onSuccess: () => toast.success(t("cellular.traffic.settings.cleared")),
    onError: (e) => toast.error(errorMessage(e)),
    onSettled: refresh,
  });

  const { errors, dirtyFields, isDirty } = form.formState;
  const submit = form.handleSubmit((v) => {
    const changes: Change[] = [];
    if (dirtyFields.reset_day) changes.push(["reset_day", v.reset_day]);
    if (dirtyFields.cap || dirtyFields.unit)
      changes.push(["cap", String(capBytes(v.cap, v.unit))]);
    if (dirtyFields.warn) changes.push(["warn", v.warn]);
    if (dirtyFields.cut) changes.push(["cut", v.cut ? "on" : "off"]);
    if (dirtyFields.plan) changes.push(["plan", v.plan]);
    if (changes.length === 0) return toast.info(t("cellular.shared.noChanges"));
    save.mutate(changes);
  });

  return (
    <Card>
      <CardHeader>
        <CardTitle>{t("cellular.traffic.settings.title")}</CardTitle>
        <CardDescription>
          {t("cellular.traffic.settings.description")}
        </CardDescription>
      </CardHeader>
      <CardContent>
        <form id="traffic-settings" onSubmit={submit}>
          <FieldGroup>
            <Field data-invalid={!!errors.plan}>
              <FieldLabel htmlFor="traffic-plan">
                {t("cellular.traffic.settings.plan")}
              </FieldLabel>
              <Input
                id="traffic-plan"
                autoComplete="off"
                aria-invalid={!!errors.plan}
                {...form.register("plan")}
              />
              <FieldError errors={[errors.plan]} />
            </Field>
            <div className="grid gap-4 sm:grid-cols-2">
              <Field data-invalid={!!errors.reset_day}>
                <FieldLabel htmlFor="traffic-reset-day">
                  {t("cellular.traffic.settings.resetDay")}
                </FieldLabel>
                <InputGroup>
                  <InputGroupInput
                    id="traffic-reset-day"
                    inputMode="numeric"
                    aria-invalid={!!errors.reset_day}
                    {...form.register("reset_day")}
                  />
                  <InputGroupAddon align="inline-end">
                    <InputGroupText>
                      {t("cellular.traffic.settings.daySuffix")}
                    </InputGroupText>
                  </InputGroupAddon>
                </InputGroup>
                <FieldDescription>
                  {t("cellular.traffic.settings.resetDayHint")}
                </FieldDescription>
                <FieldError errors={[errors.reset_day]} />
              </Field>
              <Field data-invalid={!!errors.warn}>
                <FieldLabel htmlFor="traffic-warn">
                  {t("cellular.traffic.settings.warn")}
                </FieldLabel>
                <InputGroup>
                  <InputGroupInput
                    id="traffic-warn"
                    inputMode="numeric"
                    aria-invalid={!!errors.warn}
                    {...form.register("warn")}
                  />
                  <InputGroupAddon align="inline-end">
                    <InputGroupText>%</InputGroupText>
                  </InputGroupAddon>
                </InputGroup>
                <FieldError errors={[errors.warn]} />
              </Field>
            </div>
            <Field data-invalid={!!errors.cap}>
              <FieldLabel htmlFor="traffic-cap">
                {t("cellular.traffic.settings.cap")}
              </FieldLabel>
              <div className="flex flex-wrap items-center gap-2">
                <Input
                  id="traffic-cap"
                  inputMode="decimal"
                  placeholder={t("cellular.traffic.settings.unlimited")}
                  className="w-32"
                  aria-invalid={!!errors.cap}
                  {...form.register("cap")}
                />
                <Controller
                  control={form.control}
                  name="unit"
                  render={({ field }) => (
                    <ToggleGroup
                      type="single"
                      variant="outline"
                      value={field.value}
                      onValueChange={(v) => v && field.onChange(v as CapUnit)}
                    >
                      {(Object.keys(UNITS) as CapUnit[]).map((u) => (
                        <ToggleGroupItem key={u} value={u}>
                          {u}
                        </ToggleGroupItem>
                      ))}
                    </ToggleGroup>
                  )}
                />
              </div>
              <FieldDescription>
                {t("cellular.traffic.settings.capHint")}
              </FieldDescription>
              <FieldError errors={[errors.cap]} />
            </Field>
            <FieldSeparator />
            <Field orientation="horizontal" data-invalid={!!errors.cut}>
              <FieldContent>
                <FieldLabel htmlFor="traffic-cut">
                  {t("cellular.traffic.settings.cut")}
                </FieldLabel>
                <FieldDescription>
                  {t("cellular.traffic.settings.cutHint")}
                </FieldDescription>
                <FieldError errors={[errors.cut]} />
              </FieldContent>
              <Controller
                control={form.control}
                name="cut"
                render={({ field }) => (
                  <Switch
                    id="traffic-cut"
                    checked={field.value}
                    onCheckedChange={field.onChange}
                  />
                )}
              />
            </Field>
          </FieldGroup>
        </form>
      </CardContent>
      <CardFooter className="flex flex-wrap gap-2">
        <PendingButton
          type="submit"
          form="traffic-settings"
          pending={save.isPending}
          disabled={!isDirty}
        >
          {t("common.action.save")}
        </PendingButton>
        <Button
          variant="outline"
          disabled={!isDirty || save.isPending}
          onClick={() => form.reset(defaults(tr))}
        >
          {t("common.action.reset")}
        </Button>
        <ConfirmButton
          variant="outline"
          title={t("cellular.traffic.settings.clearTitle")}
          description={t("cellular.traffic.settings.clearDescription")}
          confirmText={t("cellular.traffic.settings.clearConfirm")}
          pending={clear.isPending}
          onConfirm={() => clear.mutate()}
        >
          {t("cellular.traffic.settings.clear")}
        </ConfirmButton>
      </CardFooter>
    </Card>
  );
}
