import { useMemo, useState } from "react";
import { EditButton, FormDialog } from "@/components/f50/form-dialog";
import { KvList } from "@/components/f50/kv-list";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import {
  Card,
  CardAction,
  CardContent,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import {
  Field,
  FieldDescription,
  FieldError,
  FieldLabel,
} from "@/components/ui/field";
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group";
import { t } from "@/i18n";
import { reportFormError } from "@/lib/form-error";
import { zodResolver } from "@hookform/resolvers/zod";
import { Controller, useForm, useWatch } from "react-hook-form";
import { z } from "zod";
import type { BandsReading } from "../types";
import { type OnJob, useLockSet } from "../use-lock-set";
import { BAND_LIST_RE, bandsValue } from "../values";

/** Built per call so the message is in the current language. */
const schema = () =>
  z.object({
    bands: z
      .array(z.string())
      .refine(
        (v) => BAND_LIST_RE.test(bandsValue(v.map(Number))),
        t("cellular.locks.bands.tooMany"),
      ),
  });
type Values = z.infer<ReturnType<typeof schema>>;

/**
 * An LTE or NR band lock: the locked bands on the page, the picker (a set of the supported bands, none selected =
 * automatic) in a dialog, since the full band list would otherwise take most of the page.
 */
export function BandsCard({
  rat,
  reading,
  supported,
  disabled,
  onJob,
}: {
  rat: "lte" | "nr";
  reading: BandsReading;
  supported: number[];
  disabled: boolean;
  onJob: OnJob;
}) {
  const prefix = rat === "nr" ? "n" : "B";
  const name = rat === "nr" ? "NR (5G)" : "LTE (4G)";
  const [open, setOpen] = useState(false);
  const defaults = (): Values => ({
    bands: reading.locked.filter((b) => supported.includes(b)).map(String),
  });
  const form = useForm<Values>({
    resolver: zodResolver(useMemo(() => schema(), [])),
    defaultValues: defaults(),
  });
  const set = useLockSet(onJob);
  const unsupported = reading.locked.filter((b) => !supported.includes(b));
  const picked = useWatch({ control: form.control, name: "bands" });
  const value = bandsValue(picked.map(Number));
  const current = bandsValue(reading.locked);
  const show = (v: string) =>
    v
      ? v
          .split(",")
          .map((b) => prefix + b)
          .join(" ")
      : t("cellular.locks.bands.auto");
  const error = form.formState.errors.bands;

  const submit = form.handleSubmit((v) => {
    const next = bandsValue(v.bands.map(Number));
    set.mutate(
      {
        kind: rat,
        value: next,
        title: t("cellular.locks.bands.job", { name, bands: show(next) }),
      },
      {
        onSuccess: () => setOpen(false),
        onError: (e) => reportFormError(e, form.setError, { value: "bands" }),
      },
    );
  });
  const formId = `lock-bands-${rat}`;

  return (
    <Card>
      <CardHeader>
        <CardTitle>{t("cellular.locks.bands.title", { name })}</CardTitle>
        <CardAction>
          <EditButton
            disabled={disabled}
            onClick={() => {
              form.reset(defaults());
              setOpen(true);
            }}
          />
        </CardAction>
      </CardHeader>
      <CardContent>
        <KvList
          items={[
            {
              label: t("cellular.locks.bands.lockedBands"),
              value: (
                <span className="flex flex-wrap items-center gap-2">
                  <Badge variant={current ? "default" : "secondary"}>
                    {current
                      ? t("cellular.locks.bands.locked")
                      : t("cellular.locks.bands.autoBadge")}
                  </Badge>
                  {current && (
                    <span className="break-words">{show(current)}</span>
                  )}
                </span>
              ),
            },
          ]}
        />
      </CardContent>
      <FormDialog
        open={open}
        onOpenChange={setOpen}
        title={t("cellular.locks.bands.title", { name })}
        description={t("cellular.locks.current", { value: show(current) })}
        formId={formId}
        pending={set.isPending}
        // unsupported locked bands are not in the picker, so applying as is still changes the lock
        dirty={value !== current}
        submitText={t("common.action.apply")}
        extra={
          <Button
            type="button"
            variant="ghost"
            disabled={set.isPending || picked.length === 0}
            onClick={() =>
              form.setValue("bands", [], {
                shouldDirty: true,
                shouldValidate: true,
              })
            }
          >
            {t("cellular.locks.bands.clear")}
          </Button>
        }
      >
        <form id={formId} onSubmit={submit}>
          <Field data-invalid={!!error}>
            <FieldLabel>{t("cellular.locks.bands.pick")}</FieldLabel>
            <Controller
              control={form.control}
              name="bands"
              render={({ field }) => (
                <ToggleGroup
                  type="multiple"
                  variant="outline"
                  size="sm"
                  className="flex-wrap"
                  value={field.value}
                  disabled={set.isPending}
                  aria-invalid={!!error}
                  onValueChange={field.onChange}
                >
                  {supported.map((b) => (
                    <ToggleGroupItem
                      key={b}
                      value={String(b)}
                      aria-label={`${prefix}${b}`}
                    >
                      {prefix}
                      {b}
                    </ToggleGroupItem>
                  ))}
                </ToggleGroup>
              )}
            />
            <FieldDescription>
              {t("cellular.locks.bands.hint")}
              {unsupported.length > 0 &&
                t("cellular.locks.bands.unsupported", {
                  bands: unsupported.map((b) => prefix + b).join(" "),
                })}
            </FieldDescription>
            <FieldError errors={[error]} />
          </Field>
        </form>
      </FormDialog>
    </Card>
  );
}
