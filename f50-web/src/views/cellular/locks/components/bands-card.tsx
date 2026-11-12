import { useState } from "react";
import { PendingButton } from "@/components/f50/pending-button";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
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
  FieldLabel,
} from "@/components/ui/field";
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group";
import { t } from "@/i18n";
import { reportFormError } from "@/lib/form-error";
import { errorMessage } from "@/lib/format";
import { ApiError } from "@/lib/ubus";
import type { BandsReading } from "../types";
import { type OnJob, useLockSet } from "../use-lock-set";
import { BAND_LIST_RE, bandsValue } from "../values";

/** An LTE or NR band lock: a set of the supported bands, none selected = automatic. */
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
  const [picked, setPicked] = useState<string[]>(
    reading.locked.filter((b) => supported.includes(b)).map(String),
  );
  const [error, setError] = useState<string>();
  const set = useLockSet(onJob);
  const unsupported = reading.locked.filter((b) => !supported.includes(b));
  const value = bandsValue(picked.map(Number));
  const current = bandsValue(reading.locked);
  const show = (v: string) =>
    v
      ? v
          .split(",")
          .map((b) => prefix + b)
          .join(" ")
      : t("cellular.locks.bands.auto");

  const apply = () => {
    if (!BAND_LIST_RE.test(value))
      return setError(t("cellular.locks.bands.tooMany"));
    setError(undefined);
    set.mutate(
      {
        kind: rat,
        value,
        title: t("cellular.locks.bands.job", { name, bands: show(value) }),
      },
      {
        onError: (e) => {
          if (e instanceof ApiError && e.field === "value")
            setError(errorMessage(e));
          else reportFormError(e);
        },
      },
    );
  };

  return (
    <Card>
      <CardHeader>
        <CardTitle>{t("cellular.locks.bands.title", { name })}</CardTitle>
        <CardDescription className="break-words">
          {t("cellular.locks.current", { value: show(current) })}
        </CardDescription>
        <CardAction>
          <Badge variant={current ? "default" : "secondary"}>
            {current
              ? t("cellular.locks.bands.locked")
              : t("cellular.locks.bands.autoBadge")}
          </Badge>
        </CardAction>
      </CardHeader>
      <CardContent>
        <Field data-invalid={!!error}>
          <FieldLabel>{t("cellular.locks.bands.pick")}</FieldLabel>
          <ToggleGroup
            type="multiple"
            variant="outline"
            size="sm"
            className="flex-wrap"
            value={picked}
            disabled={disabled || set.isPending}
            aria-invalid={!!error}
            onValueChange={(v) => {
              setPicked(v);
              setError(undefined);
            }}
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
          <FieldDescription>
            {t("cellular.locks.bands.hint")}
            {unsupported.length > 0 &&
              t("cellular.locks.bands.unsupported", {
                bands: unsupported.map((b) => prefix + b).join(" "),
              })}
          </FieldDescription>
          <FieldError>{error}</FieldError>
        </Field>
      </CardContent>
      <CardFooter className="flex flex-wrap gap-2">
        <PendingButton
          pending={set.isPending}
          disabled={disabled || value === current}
          onClick={apply}
        >
          {t("common.action.apply")}
        </PendingButton>
        <Button
          variant="outline"
          disabled={disabled || set.isPending || picked.length === 0}
          onClick={() => setPicked([])}
        >
          {t("cellular.locks.bands.clear")}
        </Button>
      </CardFooter>
    </Card>
  );
}
