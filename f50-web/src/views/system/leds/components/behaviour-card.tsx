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
import { useLedSet } from "./use-led-set";
import type { LedSettings, ThermalLight } from "../types";

const THERMAL: { value: ThermalLight; label: string }[] = [
  { value: "siren", label: "警灯" },
  { value: "blink", label: "红灯闪烁" },
  { value: "off", label: "不显示" },
];

function BootSwitch({ on }: { on: boolean }) {
  const set = useLedSet("boot");
  const shown =
    set.isPending && set.variables != null ? set.variables === "1" : on;
  return (
    <Field orientation="horizontal">
      <FieldContent>
        <FieldLabel htmlFor="led-boot">开机指示</FieldLabel>
        <FieldDescription>
          开机过程中轮流点亮各颜色，直到启动完成（最长 3 分钟）。
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
      <FieldLabel>过热告警显示</FieldLabel>
      <FieldDescription>
        只决定告警的灯光效果；何时告警由温控守护决定。
      </FieldDescription>
      <ToggleGroup
        type="single"
        variant="outline"
        spacing={0}
        value={shown}
        disabled={set.isPending}
        aria-busy={set.isPending}
        aria-label="过热告警显示"
        onValueChange={(v) => {
          if (v && v !== value) set.mutate(v);
        }}
      >
        {THERMAL.map((t) => (
          <ToggleGroupItem key={t.value} value={t.value} className="px-4">
            {set.isPending && set.variables === t.value ? (
              <SweepShine>{t.label}</SweepShine>
            ) : (
              t.label
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
        <CardTitle>提示效果</CardTitle>
        <CardDescription>开机指示与过热告警</CardDescription>
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
