import { useState } from "react";
import { PendingButton } from "@/components/f50/pending-button";
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert";
import {
  Card,
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
import { Switch } from "@/components/ui/switch";
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group";
import { reportFormError } from "@/lib/form-error";
import { zodResolver } from "@hookform/resolvers/zod";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { Controller, useForm, useWatch } from "react-hook-form";
import { toast } from "sonner";
import { z } from "zod";
import { ConfirmDialog } from "../../shared/confirm-dialog";
import { setUsb, usbKeys } from "../api";
import type { UsbInfo, UsbSetArgs } from "../types";

const schema = z.object({
  role: z.enum(["host", "device"]),
  at_boot: z.boolean(),
});
type RoleForm = z.infer<typeof schema>;

/** The port's role now, and the host role at every boot (device is the boot default). */
export function UsbRoleCard({ info }: { info: UsbInfo }) {
  const qc = useQueryClient();
  const initial: RoleForm = {
    role: info.role === "host" ? "host" : "device",
    at_boot: info.role_boot === "host",
  };
  const form = useForm<RoleForm>({
    resolver: zodResolver(schema),
    defaultValues: initial,
  });
  const [pending, setPending] = useState<UsbSetArgs | null>(null);
  const save = useMutation({
    mutationFn: setUsb,
    onSuccess: (r) => {
      toast.success(r.pending ? "已保存，端口即将切换角色" : "已保存");
      qc.invalidateQueries({ queryKey: usbKeys.all });
      // the switch runs a moment after the reply
      if (r.pending)
        setTimeout(() => qc.invalidateQueries({ queryKey: usbKeys.all }), 1500);
    },
    onError: (e) =>
      reportFormError(e, form.setError, { role: "role", scope: "at_boot" }),
  });
  const role = useWatch({ control: form.control, name: "role" });
  const supported = info.host_supported && info.role !== "unavailable";

  const onSubmit = (v: RoleForm) => {
    if (
      v.role === initial.role &&
      (v.role === "device" || v.at_boot === initial.at_boot)
    ) {
      toast.info("没有需要保存的修改");
      return;
    }
    const args: UsbSetArgs =
      v.role === "host"
        ? { role: "host", scope: v.at_boot ? "permanent" : "once" }
        : { role: "device" };
    if (v.role !== initial.role) setPending(args);
    else save.mutate(args);
  };

  return (
    <Card>
      <CardHeader>
        <CardTitle>端口角色</CardTitle>
        <CardDescription>
          设备：作为 USB 网卡连接电脑；主机：接入 USB 有线网卡（需外部供电的
          Hub）。
        </CardDescription>
      </CardHeader>
      <form onSubmit={form.handleSubmit(onSubmit)} noValidate>
        <CardContent className="flex flex-col gap-4">
          {!supported && (
            <Alert>
              <AlertTitle>不支持切换角色</AlertTitle>
              <AlertDescription>此内核没有 USB 角色切换接口。</AlertDescription>
            </Alert>
          )}
          <FieldGroup>
            <Controller
              control={form.control}
              name="role"
              render={({ field, fieldState }) => (
                <Field
                  data-invalid={fieldState.invalid}
                  data-disabled={!supported || undefined}
                >
                  <FieldLabel>角色</FieldLabel>
                  <ToggleGroup
                    type="single"
                    variant="outline"
                    size="sm"
                    disabled={!supported}
                    value={field.value}
                    aria-invalid={fieldState.invalid}
                    onValueChange={(v) => v && field.onChange(v)}
                  >
                    <ToggleGroupItem value="device">设备</ToggleGroupItem>
                    <ToggleGroupItem value="host">主机</ToggleGroupItem>
                  </ToggleGroup>
                  <FieldError errors={[fieldState.error]} />
                </Field>
              )}
            />
            <Controller
              control={form.control}
              name="at_boot"
              render={({ field, fieldState }) => (
                <Field
                  orientation="horizontal"
                  data-invalid={fieldState.invalid}
                  data-disabled={role !== "host" || undefined}
                >
                  <Switch
                    id="usb-at-boot"
                    checked={role === "host" && field.value}
                    disabled={!supported || role !== "host"}
                    onCheckedChange={field.onChange}
                    aria-invalid={fieldState.invalid}
                  />
                  <FieldLabel htmlFor="usb-at-boot">
                    每次开机都使用主机模式
                  </FieldLabel>
                  <FieldError errors={[fieldState.error]} />
                </Field>
              )}
            />
            <FieldDescription>
              开机主机模式会清除 USB
              网络模式设置；选择“设备”会取消开机主机模式。
            </FieldDescription>
          </FieldGroup>
        </CardContent>
        <CardFooter className="mt-4 flex justify-end">
          <PendingButton
            type="submit"
            pending={save.isPending}
            disabled={!supported}
          >
            {save.isPending ? "保存中" : "应用"}
          </PendingButton>
        </CardFooter>
      </form>
      <ConfirmDialog
        open={pending !== null}
        onOpenChange={(o) => !o && setPending(null)}
        title={
          pending?.role === "host" ? "切换到主机模式？" : "切换到设备模式？"
        }
        confirmText="切换"
        destructive
        description={
          pending?.role === "host" ? (
            <span>
              USB 网络和 adb 会断开：如果你正通过 USB
              访问控制台，将无法继续访问，请改用 Wi-Fi。F50 无法为 USB
              设备供电，请使用带电源的 Hub。
            </span>
          ) : (
            <span>USB 网卡将从局域网移除，端口恢复为电脑的 USB 网络。</span>
          )
        }
        onConfirm={() => pending && save.mutate(pending)}
      />
    </Card>
  );
}
