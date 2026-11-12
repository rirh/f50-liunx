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
import { Spinner } from "@/components/ui/spinner";
import { SweepShine } from "@/components/ui/sweep-shine";
import { Switch } from "@/components/ui/switch";
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group";
import { type Key, t } from "@/i18n";
import { useLedSet } from "./use-led-set";
import type { LedSettings, ThermalLight } from "../types";

const THERMAL: { value: ThermalLight; label: Key }[] = [
  { value: "siren", label: "system.leds.behaviour.thermalOption.siren" },
  { value: "blink", label: "system.leds.behaviour.thermalOption.blink" },
  { value: "off", label: "system.leds.behaviour.thermalOption.off" },
];

function BootSwitch({ on }: { on: boolean }) {
  const set = useLedSet("boot");
  const shown =
    set.isPending && set.variables != null ? set.variables === "1" : on;
  return (
    <Field orientation="horizontal">
      <FieldContent>
        <FieldLabel htmlFor="led-boot">
          {t("system.leds.behaviour.boot")}
        </FieldLabel>
        <FieldDescription>
          {t("system.leds.behaviour.bootHint")}
        </FieldDescription>
      </FieldContent>
      {set.isPending && <Spinner />}
      <Switch
        id="led-boot"
        checked={shown}
        disabled={set.isPending}
        aria-busy={set.isPending}
        onCheckedChange={(c) => set.mutate(c ? "1" : "0")}
      />
    </Field>
  );
}

function ThermalChoice({ value }: { value: ThermalLight }) {
  const set = useLedSet("thermal");
  const shown = set.isPending && set.variables ? set.variables : value;
  return (
    <Field>
      <FieldLabel>{t("system.leds.behaviour.thermal")}</FieldLabel>
      <FieldDescription>
        {t("system.leds.behaviour.thermalHint")}
      </FieldDescription>
      <ToggleGroup
        type="single"
        variant="outline"
        spacing={0}
        value={shown}
        disabled={set.isPending}
        aria-busy={set.isPending}
        aria-label={t("system.leds.behaviour.thermal")}
        onValueChange={(v) => {
          if (v && v !== value) set.mutate(v);
        }}
      >
        {THERMAL.map((o) => (
          <ToggleGroupItem key={o.value} value={o.value} className="px-4">
            {set.isPending && set.variables === o.value ? (
              <SweepShine>{t(o.label)}</SweepShine>
            ) : (
              t(o.label)
            )}
          </ToggleGroupItem>
        ))}
      </ToggleGroup>
    </Field>
  );
}

export function BehaviourCard({ data }: { data: LedSettings }) {
  return (
    <Card>
      <CardHeader>
        <CardTitle>{t("system.leds.behaviour.title")}</CardTitle>
        <CardDescription>
          {t("system.leds.behaviour.description")}
        </CardDescription>
      </CardHeader>
      <CardContent>
        <FieldGroup>
          <BootSwitch on={data.boot} />
          <FieldSeparator />
          <ThermalChoice value={data.thermal} />
        </FieldGroup>
      </CardContent>
    </Card>
  );
}
