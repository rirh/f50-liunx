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
import { reportFormError } from "@/lib/form-error";
import { formatDuration } from "@/lib/format";
import { zodResolver } from "@hookform/resolvers/zod";
import { Controller, useForm } from "react-hook-form";
import { useLedSet } from "./use-led-set";
import { type TimeoutValues, timeoutSchema } from "../schemas";
import type { LedSettings } from "../types";

const describe = (s: number) =>
  s === 0 ? "常亮" : `${formatDuration(s)}无操作后熄灭`;

export function TimeoutCard({ data }: { data: LedSettings }) {
  const form = useForm<TimeoutValues>({
    resolver: zodResolver(timeoutSchema),
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
        <CardTitle>自动熄灭</CardTitle>
        <CardDescription>当前：{describe(data.timeout)}</CardDescription>
        <CardAction>
          {data.timeout_default ? (
            <Badge variant="secondary">设备默认</Badge>
          ) : (
            <Badge variant="outline">自定义</Badge>
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
                  <FieldLabel htmlFor="led-timeout">熄灭延时</FieldLabel>
                  <InputGroup>
                    <InputGroupInput
                      {...field}
                      id="led-timeout"
                      inputMode="numeric"
                      autoComplete="off"
                      aria-invalid={fieldState.invalid}
                    />
                    <InputGroupAddon align="inline-end">
                      <InputGroupText>秒</InputGroupText>
                    </InputGroupAddon>
                  </InputGroup>
                  <FieldDescription>
                    按键后经过这么久指示灯熄灭；0 表示常亮，最长 86400
                    秒。设备默认 {data.default_timeout} 秒（
                    {describe(data.default_timeout)}）。
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
            保存
          </PendingButton>
          <PendingButton
            type="button"
            variant="outline"
            pending={reset.isPending}
            disabled={busy || data.timeout_default}
            onClick={() => reset.mutate("default")}
          >
            恢复默认
          </PendingButton>
        </CardFooter>
      </form>
    </Card>
  );
}
