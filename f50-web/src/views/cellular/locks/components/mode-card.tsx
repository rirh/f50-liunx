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
  const label = (m: ModeLabel) => MODES.find((x) => x.value === m)?.label ?? m;
  return (
    <Card>
      <CardHeader>
        <CardTitle>网络模式</CardTitle>
        <CardDescription>
          更改后协议栈重启，蜂窝连接中断约半分钟
        </CardDescription>
        <CardAction>
          <Badge variant="secondary">当前：{label(current)}</Badge>
        </CardAction>
      </CardHeader>
      <CardContent>
        <Field>
          <FieldLabel>模式</FieldLabel>
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
                {m.label}
              </ToggleGroupItem>
            ))}
          </ToggleGroup>
          <FieldDescription>
            {MODES.find((m) => m.value === mode)?.hint}
          </FieldDescription>
        </Field>
      </CardContent>
      <CardFooter>
        <PendingButton
          pending={set.isPending}
          disabled={disabled || mode === current}
          onClick={() =>
            set.mutate(
              { kind: "mode", value: mode, title: `网络模式 → ${label(mode)}` },
              { onError: (e) => reportFormError(e) },
            )
          }
        >
          应用
        </PendingButton>
      </CardFooter>
    </Card>
  );
}
