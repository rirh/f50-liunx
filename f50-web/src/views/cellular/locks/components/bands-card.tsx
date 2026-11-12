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
      : "自动（全部频段）";

  const apply = () => {
    if (!BAND_LIST_RE.test(value))
      return setError("最多 32 个频段，每个 1–3 位数字");
    setError(undefined);
    set.mutate(
      { kind: rat, value, title: `${name} 频段 → ${show(value)}` },
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
        <CardTitle>{name} 频段锁定</CardTitle>
        <CardDescription className="break-words">
          当前：{show(current)}
        </CardDescription>
        <CardAction>
          <Badge variant={current ? "default" : "secondary"}>
            {current ? "已锁定" : "自动"}
          </Badge>
        </CardAction>
      </CardHeader>
      <CardContent>
        <Field data-invalid={!!error}>
          <FieldLabel>选择频段</FieldLabel>
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
            不选任何频段即为自动。仅列出设备支持的频段。
            {unsupported.length > 0 &&
              ` 当前锁定中含不支持的频段：${unsupported.map((b) => prefix + b).join(" ")}。`}
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
          应用
        </PendingButton>
        <Button
          variant="outline"
          disabled={disabled || set.isPending || picked.length === 0}
          onClick={() => setPicked([])}
        >
          清空（自动）
        </Button>
      </CardFooter>
    </Card>
  );
}
