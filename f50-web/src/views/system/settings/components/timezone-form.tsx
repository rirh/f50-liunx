import { useMemo } from "react";
import { PendingButton } from "@/components/f50/pending-button";
import {
  Card,
  CardContent,
  CardDescription,
  CardFooter,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import {
  Combobox,
  ComboboxContent,
  ComboboxEmpty,
  ComboboxInput,
  ComboboxItem,
  ComboboxList,
} from "@/components/ui/combobox";
import {
  Field,
  FieldDescription,
  FieldError,
  FieldGroup,
  FieldLabel,
} from "@/components/ui/field";
import { reportFormError } from "@/lib/form-error";
import { formatTime } from "@/lib/format";
import { zodResolver } from "@hookform/resolvers/zod";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { Controller, useForm } from "react-hook-form";
import { toast } from "sonner";
import { infoKey, setTimezone } from "../api";
import { type TimezoneValues, timezoneSchema, zoneList } from "../schemas";

export function TimezoneForm({
  current,
  time,
}: {
  current: string | null;
  time: number;
}) {
  const qc = useQueryClient();
  const zones = useMemo(() => {
    const list = zoneList();
    return current && !list.includes(current) ? [current, ...list] : list;
  }, [current]);
  const form = useForm<TimezoneValues>({
    resolver: zodResolver(timezoneSchema),
    values: { zone: current ?? "UTC" },
  });
  const save = useMutation({
    mutationFn: (v: TimezoneValues) => setTimezone(v.zone),
    onSuccess: (r) => {
      toast.success(`时区已改为 ${r.timezone}`);
      qc.invalidateQueries({ queryKey: infoKey });
    },
    onError: (e) => reportFormError(e, form.setError, { zone: "zone" }),
  });
  return (
    <Card>
      <CardHeader>
        <CardTitle>时区</CardTitle>
        <CardDescription>
          设备时间：{formatTime(time)}（按浏览器时区显示）
        </CardDescription>
      </CardHeader>
      <form
        onSubmit={form.handleSubmit((v) => save.mutate(v))}
        className="contents"
      >
        <CardContent>
          <FieldGroup>
            <Controller
              name="zone"
              control={form.control}
              render={({ field, fieldState }) => (
                <Field data-invalid={fieldState.invalid}>
                  <FieldLabel htmlFor="timezone">时区</FieldLabel>
                  <Combobox
                    items={zones}
                    value={field.value}
                    onValueChange={(v) =>
                      field.onChange((v as string | null) ?? "")
                    }
                  >
                    <ComboboxInput
                      id="timezone"
                      placeholder="搜索时区，如 Asia/Shanghai"
                      aria-invalid={fieldState.invalid}
                      onBlur={field.onBlur}
                    />
                    <ComboboxContent>
                      <ComboboxEmpty>没有匹配的时区</ComboboxEmpty>
                      <ComboboxList>
                        {(z: string) => (
                          <ComboboxItem key={z} value={z}>
                            {z}
                          </ComboboxItem>
                        )}
                      </ComboboxList>
                    </ComboboxContent>
                  </Combobox>
                  <FieldDescription>
                    IANA 时区名（如 Asia/Shanghai）或
                    UTC。设备上缺少该时区数据时会被拒绝。
                  </FieldDescription>
                  {fieldState.invalid && (
                    <FieldError errors={[fieldState.error]} />
                  )}
                </Field>
              )}
            />
          </FieldGroup>
        </CardContent>
        <CardFooter>
          <PendingButton
            type="submit"
            pending={save.isPending}
            disabled={!form.formState.isDirty}
          >
            保存
          </PendingButton>
        </CardFooter>
      </form>
    </Card>
  );
}
