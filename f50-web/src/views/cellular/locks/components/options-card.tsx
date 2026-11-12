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
import { Switch } from "@/components/ui/switch";
import { reportFormError } from "@/lib/form-error";
import { type OnJob, useLockSet } from "../use-lock-set";

/** EN-DC (a job, no stack restart) and the boot replay switch (saved at once). */
export function OptionsCard({
  endc,
  autoApply,
  disabled,
  onJob,
}: {
  endc: boolean | null;
  autoApply: boolean;
  disabled: boolean;
  onJob: OnJob;
}) {
  const endcSet = useLockSet(onJob);
  const replaySet = useLockSet(onJob);
  return (
    <Card>
      <CardHeader>
        <CardTitle>其他选项</CardTitle>
        <CardDescription>EN-DC 与开机重放</CardDescription>
      </CardHeader>
      <CardContent>
        <FieldGroup>
          <Field
            orientation="horizontal"
            data-disabled={disabled || endcSet.isPending}
          >
            <FieldContent>
              <FieldLabel htmlFor="lock-endc">EN-DC（4G+5G 双连接）</FieldLabel>
              <FieldDescription>
                {endcSet.isPending
                  ? "正在提交…"
                  : endc == null
                    ? "未能读取当前状态"
                    : "5G NSA 需要开启；无需重启协议栈"}
              </FieldDescription>
            </FieldContent>
            <Switch
              id="lock-endc"
              checked={endc ?? false}
              disabled={disabled || endcSet.isPending}
              aria-busy={endcSet.isPending}
              onCheckedChange={(on) =>
                endcSet.mutate(
                  {
                    kind: "endc",
                    value: on ? "on" : "off",
                    title: on ? "开启 EN-DC" : "关闭 EN-DC",
                  },
                  { onError: (e) => reportFormError(e) },
                )
              }
            />
          </Field>
          <FieldSeparator />
          <Field orientation="horizontal" data-disabled={replaySet.isPending}>
            <FieldContent>
              <FieldLabel htmlFor="lock-auto-apply">启动时自动应用</FieldLabel>
              <FieldDescription>
                {replaySet.isPending
                  ? "正在保存…"
                  : "开机时重新应用已保存的锁定（小区锁定只能靠它恢复）"}
              </FieldDescription>
            </FieldContent>
            <Switch
              id="lock-auto-apply"
              checked={autoApply}
              disabled={replaySet.isPending}
              aria-busy={replaySet.isPending}
              onCheckedChange={(on) =>
                replaySet.mutate(
                  { kind: "auto_apply", value: on ? "on" : "off", title: "" },
                  { onError: (e) => reportFormError(e) },
                )
              }
            />
          </Field>
        </FieldGroup>
      </CardContent>
    </Card>
  );
}
