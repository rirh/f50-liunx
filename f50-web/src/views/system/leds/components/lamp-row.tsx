import { useState } from "react";
import {
  Field,
  FieldContent,
  FieldDescription,
  FieldLabel,
} from "@/components/ui/field";
import { Slider } from "@/components/ui/slider";
import { Spinner } from "@/components/ui/spinner";
import { Switch } from "@/components/ui/switch";
import { useLedSet } from "./use-led-set";
import type { Lamp } from "../types";

const LAMP_TEXT: Record<Lamp, { label: string; description: string }> = {
  network: {
    label: "网络指示灯",
    description:
      "蓝色：2G/3G/4G 数据；白色：5G 数据；红色：无服务；红色闪烁：正在搜索网络。",
  },
  wifi: { label: "Wi-Fi 指示灯", description: "热点开启时点亮。" },
};

/** The brightness slider: moves locally, saves on release. Remount (key) when the saved value changes. */
function Brightness({
  lamp,
  value,
  disabled,
}: {
  lamp: Lamp;
  value: number;
  disabled: boolean;
}) {
  const [v, setV] = useState(value);
  const set = useLedSet(
    lamp === "network" ? "network_brightness" : "wifi_brightness",
  );
  const id = `${lamp}-brightness`;
  return (
    <Field data-disabled={disabled || undefined}>
      <div className="flex items-center justify-between gap-2">
        <FieldLabel htmlFor={id}>亮度</FieldLabel>
        <span
          className="flex items-center gap-1.5 text-sm tabular-nums"
          aria-live="polite"
        >
          {set.isPending && <Spinner />}
          {v}%
        </span>
      </div>
      <Slider
        id={id}
        min={1}
        max={100}
        step={1}
        value={[v]}
        disabled={disabled || set.isPending}
        aria-busy={set.isPending}
        aria-label={`${LAMP_TEXT[lamp].label}亮度`}
        onValueChange={([n]) => setV(n)}
        onValueCommit={([n]) => {
          if (n !== value) set.mutate(String(n));
        }}
      />
    </Field>
  );
}

export function LampRow({
  lamp,
  on,
  brightness,
  dimmable,
}: {
  lamp: Lamp;
  on: boolean;
  brightness: number;
  dimmable: boolean;
}) {
  const set = useLedSet(lamp);
  const id = `${lamp}-switch`;
  const shown =
    set.isPending && set.variables != null ? set.variables === "1" : on;
  return (
    <div className="flex flex-col gap-4 rounded-lg border p-4">
      <Field orientation="horizontal">
        <FieldContent>
          <FieldLabel htmlFor={id}>{LAMP_TEXT[lamp].label}</FieldLabel>
          <FieldDescription>
            {LAMP_TEXT[lamp].description}关闭后保持熄灭（过热告警仍会显示）。
          </FieldDescription>
        </FieldContent>
        {set.isPending && <Spinner />}
        <Switch
          id={id}
          checked={shown}
          disabled={set.isPending}
          aria-busy={set.isPending}
          onCheckedChange={(c) => set.mutate(c ? "1" : "0")}
        />
      </Field>
      {dimmable && (
        <Brightness
          key={brightness}
          lamp={lamp}
          value={brightness}
          disabled={!on}
        />
      )}
    </div>
  );
}
