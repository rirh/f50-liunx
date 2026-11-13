import { type ReactNode, useMemo, useState } from "react";
import { Button } from "@/components/ui/button";
import { SweepShine } from "@/components/ui/sweep-shine";
import { t } from "@/i18n";
import { reportFormError } from "@/lib/form-error";
import { formatDuration } from "@/lib/format";
import { cn } from "@/lib/utils";
import { zodResolver } from "@hookform/resolvers/zod";
import { useForm } from "react-hook-form";
import { AppleSwitch, Segmented } from "./apple-controls";
import { useLedSet } from "./use-led-set";
import { type TimeoutValues, timeoutSchema } from "../schemas";
import type { LedSettings, ThermalLight } from "../types";

/** An inset grouped list, as in iOS Settings: a small heading, rounded rows, a footnote. */
function Group({
  title,
  footer,
  children,
}: {
  title: string;
  footer?: ReactNode;
  children: ReactNode;
}) {
  return (
    <section className="flex flex-col gap-1.5">
      <h3 className="px-4 text-[13px] text-muted-foreground uppercase">
        {title}
      </h3>
      <div className="divide-y divide-border/70 overflow-hidden rounded-xl bg-card ring-1 ring-border/60">
        {children}
      </div>
      {footer && (
        <p className="px-4 text-[13px] leading-snug text-muted-foreground">
          {footer}
        </p>
      )}
    </section>
  );
}

function Row({
  label,
  hint,
  children,
  stacked,
}: {
  label: string;
  hint?: string;
  children: ReactNode;
  stacked?: boolean;
}) {
  return (
    <div
      className={cn(
        "flex min-h-12 gap-3 px-4 py-2.5",
        stacked ? "flex-col" : "items-center justify-between",
      )}
    >
      <div className="flex min-w-0 flex-col">
        <span className="text-[15px]">{label}</span>
        {hint && (
          <span className="text-[13px] leading-snug text-muted-foreground">
            {hint}
          </span>
        )}
      </div>
      {children}
    </div>
  );
}

function BootRow({ on }: { on: boolean }) {
  const set = useLedSet("boot");
  const shown =
    set.isPending && set.variables != null ? set.variables === "1" : on;
  return (
    <Row
      label={t("system.leds.behaviour.boot")}
      hint={t("system.leds.behaviour.bootHint")}
    >
      <AppleSwitch
        checked={shown}
        disabled={set.isPending}
        aria-busy={set.isPending}
        aria-label={t("system.leds.behaviour.boot")}
        onCheckedChange={(c) => set.mutate(c ? "1" : "0")}
      />
    </Row>
  );
}

function ThermalRow({ value }: { value: ThermalLight }) {
  const set = useLedSet("thermal");
  const shown = (set.isPending && set.variables) || value;
  const label = (v: ThermalLight) => {
    const text = t(`system.leds.behaviour.thermalOption.${v}`);
    return set.isPending && set.variables === v ? (
      <SweepShine>{text}</SweepShine>
    ) : (
      text
    );
  };
  return (
    <Row
      stacked
      label={t("system.leds.behaviour.thermal")}
      hint={t("system.leds.behaviour.thermalHint")}
    >
      <Segmented<ThermalLight>
        label={t("system.leds.behaviour.thermal")}
        value={shown as ThermalLight}
        disabled={set.isPending}
        options={(["siren", "blink", "off"] as const).map((v) => ({
          value: v,
          label: label(v),
        }))}
        onChange={(v) => set.mutate(v)}
      />
    </Row>
  );
}

type Preset = "0" | "30" | "60" | "300" | "custom";
const PRESETS: { value: Preset; label: string }[] = [
  { value: "0", label: "system.leds.timeout.preset.always" },
  { value: "30", label: "system.leds.timeout.preset.s30" },
  { value: "60", label: "system.leds.timeout.preset.m1" },
  { value: "300", label: "system.leds.timeout.preset.m5" },
  { value: "custom", label: "system.leds.timeout.preset.custom" },
];

function TimeoutRows({ data }: { data: LedSettings }) {
  const preset: Preset =
    (["0", "30", "60", "300"] as const).find(
      (p) => p === String(data.timeout),
    ) ?? "custom";
  const [custom, setCustom] = useState(preset === "custom");
  const schema = useMemo(() => timeoutSchema(), []);
  const form = useForm<TimeoutValues>({
    resolver: zodResolver(schema),
    values: { seconds: String(data.timeout) },
  });
  const set = useLedSet("timeout", (e) =>
    reportFormError(e, form.setError, { value: "seconds" }),
  );
  const shown: Preset = custom
    ? "custom"
    : set.isPending && set.variables && set.variables !== "default"
      ? ((PRESETS.find((p) => p.value === set.variables)?.value ??
          "custom") as Preset)
      : preset;
  const err = form.formState.errors.seconds?.message;
  const describe = (s: number) =>
    s === 0
      ? t("system.leds.timeout.always")
      : t("system.leds.timeout.after", { duration: formatDuration(s) });

  return (
    <>
      <Row
        stacked
        label={t("system.leds.timeout.label")}
        hint={describe(data.timeout)}
      >
        <Segmented<Preset>
          label={t("system.leds.timeout.label")}
          value={shown}
          disabled={set.isPending}
          options={PRESETS.map((p) => ({
            value: p.value,
            // the keys are literal above; the cast keeps t's key type
            label: t(p.label as Parameters<typeof t>[0]),
          }))}
          onChange={(v) => {
            if (v === "custom") return setCustom(true);
            setCustom(false);
            set.mutate(v);
          }}
        />
      </Row>
      {shown === "custom" && (
        <form
          onSubmit={form.handleSubmit((v) => set.mutate(v.seconds))}
          className="flex flex-col gap-1 px-4 py-2.5"
        >
          <div className="flex items-center gap-2">
            <input
              {...form.register("seconds")}
              inputMode="numeric"
              autoComplete="off"
              aria-label={t("system.leds.timeout.label")}
              aria-invalid={!!err}
              className="h-9 min-w-0 flex-1 rounded-[9px] bg-foreground/[0.06] px-3 text-[15px] tabular-nums outline-none focus-visible:ring-2 focus-visible:ring-ring/50 aria-invalid:ring-2 aria-invalid:ring-destructive/50"
            />
            <span className="text-[15px] text-muted-foreground">
              {t("system.leds.timeout.seconds")}
            </span>
            <Button
              type="submit"
              size="sm"
              className="rounded-full px-4"
              disabled={set.isPending || !form.formState.isDirty}
            >
              {set.isPending ? (
                <SweepShine>{t("system.leds.timeout.save")}</SweepShine>
              ) : (
                t("system.leds.timeout.save")
              )}
            </Button>
          </div>
          {err && <span className="text-[13px] text-destructive">{err}</span>}
        </form>
      )}
      {!data.timeout_default && (
        <button
          type="button"
          disabled={set.isPending}
          onClick={() => {
            setCustom(false);
            set.mutate("default");
          }}
          className="flex min-h-11 w-full items-center px-4 text-left text-[15px] text-[#0a84ff] hover:bg-muted/50 disabled:opacity-50"
        >
          {t("system.leds.timeout.restore")}
        </button>
      )}
    </>
  );
}

/** Everything below the lamps, as iOS Settings lists it. */
export function SettingsList({ data }: { data: LedSettings }) {
  return (
    <div className="grid gap-6 lg:grid-cols-2">
      <Group
        title={t("system.leds.timeout.title")}
        footer={t("system.leds.timeout.hint", {
          n: data.default_timeout,
          text:
            data.default_timeout === 0
              ? t("system.leds.timeout.always")
              : t("system.leds.timeout.after", {
                  duration: formatDuration(data.default_timeout),
                }),
        })}
      >
        <TimeoutRows data={data} />
      </Group>
      <Group title={t("system.leds.behaviour.group")}>
        <BootRow on={data.boot} />
        <ThermalRow value={data.thermal} />
      </Group>
    </div>
  );
}
