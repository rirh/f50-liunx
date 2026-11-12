import { useMemo, useState } from "react";
import { PendingButton } from "@/components/f50/pending-button";
import { Button } from "@/components/ui/button";
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
  FieldSeparator,
} from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import {
  Select,
  SelectContent,
  SelectGroup,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { Switch } from "@/components/ui/switch";
import { reportFormError } from "@/lib/form-error";
import { zodResolver } from "@hookform/resolvers/zod";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { Controller, useForm, useWatch } from "react-hook-form";
import { toast } from "sonner";
import { ConfirmDialog } from "../../shared/confirm-dialog";
import { lanKeys, setLan } from "../api";
import {
  LAN_FIELDS,
  type LanForm,
  NETMASKS,
  consoleUrlAt,
  dhcpRange,
  lanArgs,
  lanDefaults,
  lanSchema,
} from "../schema";
import type { LanInfo, LanSetArgs } from "../types";

function prefixOf(mask: string): number {
  return mask
    .split(".")
    .reduce((n, o) => n + (Number(o).toString(2).match(/1/g)?.length ?? 0), 0);
}

export function LanSettingsForm({
  info,
  onMoved,
}: {
  info: LanInfo;
  onMoved: (url: string) => void;
}) {
  const qc = useQueryClient();
  const initial = useMemo(() => lanDefaults(info), [info]);
  const form = useForm<LanForm>({
    resolver: zodResolver(lanSchema),
    defaultValues: initial,
    mode: "onTouched",
  });
  const [pending, setPending] = useState<LanSetArgs | null>(null);

  const save = useMutation({
    mutationFn: setLan,
    onSuccess: (_, args) => {
      if (args.address) {
        onMoved(consoleUrlAt(args.address));
        toast.success("已保存，约 2 秒后局域网切换到新地址");
      } else {
        toast.success("已保存，约 2 秒后生效");
        qc.invalidateQueries({ queryKey: lanKeys.info });
      }
    },
    onError: (e) => reportFormError(e, form.setError, LAN_FIELDS),
  });

  const [address, netmask, dhcp, start, limit] = useWatch({
    control: form.control,
    name: ["address", "netmask", "dhcp", "dhcp_start", "dhcp_limit"],
  });
  const range = dhcpRange(address, netmask, start, limit);

  const onSubmit = (v: LanForm) => {
    const args = lanArgs(v, initial);
    if (Object.keys(args).length === 0) {
      toast.info("没有需要保存的修改");
      return;
    }
    if (args.address || args.netmask) setPending(args);
    else save.mutate(args);
  };

  return (
    <Card>
      <CardHeader>
        <CardTitle>局域网设置</CardTitle>
        <CardDescription>
          保存后约 2 秒网络与 DHCP 服务重新加载。
        </CardDescription>
      </CardHeader>
      <form onSubmit={form.handleSubmit(onSubmit)} noValidate>
        <CardContent>
          <FieldGroup>
            <div className="grid gap-4 md:grid-cols-2">
              <Controller
                control={form.control}
                name="address"
                render={({ field, fieldState }) => (
                  <Field data-invalid={fieldState.invalid}>
                    <FieldLabel htmlFor="lan-address">路由器地址</FieldLabel>
                    <Input
                      id="lan-address"
                      inputMode="decimal"
                      autoComplete="off"
                      className="font-mono"
                      aria-invalid={fieldState.invalid}
                      {...field}
                    />
                    {fieldState.error ? (
                      <FieldError errors={[fieldState.error]} />
                    ) : (
                      <FieldDescription>
                        私有网段的 .1 地址；修改后控制台需通过新地址访问
                      </FieldDescription>
                    )}
                  </Field>
                )}
              />
              <Controller
                control={form.control}
                name="netmask"
                render={({ field, fieldState }) => (
                  <Field data-invalid={fieldState.invalid}>
                    <FieldLabel htmlFor="lan-netmask">子网掩码</FieldLabel>
                    <Select value={field.value} onValueChange={field.onChange}>
                      <SelectTrigger
                        id="lan-netmask"
                        className="w-full font-mono"
                        aria-invalid={fieldState.invalid}
                      >
                        <SelectValue />
                      </SelectTrigger>
                      <SelectContent>
                        <SelectGroup>
                          {NETMASKS.map((m) => (
                            <SelectItem key={m} value={m} className="font-mono">
                              {m} (/{prefixOf(m)})
                            </SelectItem>
                          ))}
                        </SelectGroup>
                      </SelectContent>
                    </Select>
                    <FieldError errors={[fieldState.error]} />
                  </Field>
                )}
              />
            </div>
            <FieldSeparator />
            <Controller
              control={form.control}
              name="dhcp"
              render={({ field }) => (
                <Field orientation="horizontal">
                  <Switch
                    id="lan-dhcp"
                    checked={field.value}
                    onCheckedChange={field.onChange}
                  />
                  <FieldLabel htmlFor="lan-dhcp">启用 DHCP 服务</FieldLabel>
                </Field>
              )}
            />
            <div className="grid gap-4 sm:grid-cols-3">
              {(
                [
                  ["dhcp_start", "起始偏移", "2-254", "numeric"],
                  ["dhcp_limit", "地址数量", "1-253", "numeric"],
                  ["leasetime", "租期", "如 12h、30m、infinite", "text"],
                ] as const
              ).map(([name, label, hint, mode]) => (
                <Controller
                  key={name}
                  control={form.control}
                  name={name}
                  render={({ field, fieldState }) => (
                    <Field
                      data-invalid={fieldState.invalid}
                      data-disabled={!dhcp || undefined}
                    >
                      <FieldLabel htmlFor={`lan-${name}`}>{label}</FieldLabel>
                      <Input
                        id={`lan-${name}`}
                        inputMode={mode}
                        autoComplete="off"
                        disabled={!dhcp}
                        aria-invalid={fieldState.invalid}
                        {...field}
                      />
                      {fieldState.error ? (
                        <FieldError errors={[fieldState.error]} />
                      ) : (
                        <FieldDescription>{hint}</FieldDescription>
                      )}
                    </Field>
                  )}
                />
              ))}
            </div>
            {dhcp && range && (
              <FieldDescription>
                地址池：<span className="font-mono">{range}</span>
              </FieldDescription>
            )}
          </FieldGroup>
        </CardContent>
        <CardFooter className="mt-4 flex flex-wrap justify-end gap-2">
          <Button
            type="button"
            variant="outline"
            disabled={save.isPending || !form.formState.isDirty}
            onClick={() => form.reset(initial)}
          >
            还原
          </Button>
          <PendingButton type="submit" pending={save.isPending}>
            {save.isPending ? "保存中" : "保存并应用"}
          </PendingButton>
        </CardFooter>
      </form>
      <ConfirmDialog
        open={pending !== null}
        onOpenChange={(o) => !o && setPending(null)}
        title="修改局域网地址？"
        confirmText="保存并切换"
        destructive
        description={
          <>
            <span>
              约 2
              秒后局域网切换到新网段，所有连接的设备需要重新获取地址（可重新连接
              Wi-Fi 或拔插 USB）。
            </span>
            {pending?.address ? (
              <span>
                控制台将移到：
                <span className="font-mono break-all">
                  {consoleUrlAt(pending.address)}
                </span>
              </span>
            ) : (
              <span>
                新子网掩码：
                <span className="font-mono">{pending?.netmask}</span>
              </span>
            )}
          </>
        }
        onConfirm={() => pending && save.mutate(pending)}
      />
    </Card>
  );
}
