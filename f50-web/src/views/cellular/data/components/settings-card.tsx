import { PendingButton } from "@/components/f50/pending-button";
import { Badge } from "@/components/ui/badge";
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
  FieldContent,
  FieldDescription,
  FieldError,
  FieldGroup,
  FieldLabel,
  FieldSeparator,
} from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { Switch } from "@/components/ui/switch";
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group";
import { reportFormError } from "@/lib/form-error";
import { zodResolver } from "@hookform/resolvers/zod";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { Controller, useForm, useWatch } from "react-hook-form";
import { toast } from "sonner";
import { dataKeys, setData } from "../api";
import { type DataForm, dataSchema } from "../schema";
import type { AuthWrite, DataSettings, DataStatus } from "../types";

const AUTH_NOTE = {
  both: "当前：自动（PAP 或 CHAP，由网络选择）",
  invalid: "当前保存的认证方式无效",
} as const;

function defaults(d: DataStatus): DataForm {
  return {
    apn: d.apn,
    auth: d.auth,
    username: d.username,
    password: "",
    pdp: d.pdp,
    roaming: d.roaming,
  };
}

/** The APN settings and data roaming of the SIM in use; only changed fields are sent. */
export function SettingsCard({ d }: { d: DataStatus }) {
  const qc = useQueryClient();
  const form = useForm<DataForm>({
    resolver: zodResolver(dataSchema),
    defaultValues: defaults(d),
  });
  const save = useMutation({
    mutationFn: setData,
    onSuccess: (r) => {
      toast.success(r.redial ? "已保存，正在重新拨号" : "已保存");
      qc.invalidateQueries({ queryKey: dataKeys.get });
    },
    onError: (e) => reportFormError(e, form.setError),
  });
  const { errors, dirtyFields, isDirty } = form.formState;
  const auth = useWatch({ control: form.control, name: "auth" });
  const creds = auth !== "none";

  const submit = form.handleSubmit((v) => {
    const out: DataSettings = {};
    if (dirtyFields.apn) out.apn = v.apn;
    if (dirtyFields.auth && v.auth !== "both" && v.auth !== "invalid")
      out.auth = v.auth;
    if (dirtyFields.username) out.username = v.username;
    if (v.password !== "") out.password = v.password;
    if (dirtyFields.pdp) out.pdp = v.pdp;
    if (dirtyFields.roaming) out.roaming = v.roaming;
    if (Object.keys(out).length === 0) return toast.info("没有需要保存的更改");
    save.mutate(out);
  });

  return (
    <Card>
      <CardHeader>
        <CardTitle>APN 设置</CardTitle>
        <CardDescription>
          当前 SIM{d.slot} 的接入点；更改接入参数会重新拨号
        </CardDescription>
      </CardHeader>
      <CardContent>
        <form id="data-settings" onSubmit={submit}>
          <FieldGroup>
            <Field data-invalid={!!errors.apn}>
              <FieldLabel htmlFor="data-apn">APN</FieldLabel>
              <Input
                id="data-apn"
                autoComplete="off"
                aria-invalid={!!errors.apn}
                {...form.register("apn")}
              />
              <FieldDescription>留空则使用网络默认接入点。</FieldDescription>
              <FieldError errors={[errors.apn]} />
            </Field>
            <Field data-invalid={!!errors.pdp}>
              <FieldLabel>IP 类型</FieldLabel>
              <Controller
                control={form.control}
                name="pdp"
                render={({ field }) => (
                  <ToggleGroup
                    type="single"
                    variant="outline"
                    className="flex-wrap"
                    value={field.value}
                    aria-invalid={!!errors.pdp}
                    onValueChange={(v) => v && field.onChange(v)}
                  >
                    <ToggleGroupItem value="ipv4">IPv4</ToggleGroupItem>
                    <ToggleGroupItem value="ipv6">IPv6</ToggleGroupItem>
                    <ToggleGroupItem value="ipv4v6">IPv4v6</ToggleGroupItem>
                  </ToggleGroup>
                )}
              />
              <FieldError errors={[errors.pdp]} />
            </Field>
            <Field data-invalid={!!errors.auth}>
              <FieldLabel>认证方式</FieldLabel>
              <Controller
                control={form.control}
                name="auth"
                render={({ field }) => (
                  <ToggleGroup
                    type="single"
                    variant="outline"
                    className="flex-wrap"
                    value={field.value}
                    aria-invalid={!!errors.auth}
                    onValueChange={(v) => v && field.onChange(v as AuthWrite)}
                  >
                    <ToggleGroupItem value="none">无</ToggleGroupItem>
                    <ToggleGroupItem value="pap">PAP</ToggleGroupItem>
                    <ToggleGroupItem value="chap">CHAP</ToggleGroupItem>
                  </ToggleGroup>
                )}
              />
              {(auth === "both" || auth === "invalid") && (
                <FieldDescription>{AUTH_NOTE[auth]}</FieldDescription>
              )}
              <FieldError errors={[errors.auth]} />
            </Field>
            {creds && (
              <div className="grid gap-4 sm:grid-cols-2">
                <Field data-invalid={!!errors.username}>
                  <FieldLabel htmlFor="data-username">用户名</FieldLabel>
                  <Input
                    id="data-username"
                    autoComplete="off"
                    aria-invalid={!!errors.username}
                    {...form.register("username")}
                  />
                  <FieldError errors={[errors.username]} />
                </Field>
                <Field data-invalid={!!errors.password}>
                  <FieldLabel
                    htmlFor="data-password"
                    className="flex items-center gap-2"
                  >
                    密码
                    <Badge variant={d.password_set ? "secondary" : "outline"}>
                      {d.password_set ? "已设置" : "未设置"}
                    </Badge>
                  </FieldLabel>
                  <Input
                    id="data-password"
                    type="password"
                    autoComplete="new-password"
                    placeholder={d.password_set ? "留空保持不变" : ""}
                    aria-invalid={!!errors.password}
                    {...form.register("password")}
                  />
                  <FieldError errors={[errors.password]} />
                </Field>
              </div>
            )}
            <FieldSeparator />
            <Field orientation="horizontal" data-invalid={!!errors.roaming}>
              <FieldContent>
                <FieldLabel htmlFor="data-roaming">数据漫游</FieldLabel>
                <FieldDescription>
                  {d.roaming_held
                    ? "正在漫游，数据因未允许漫游而暂停"
                    : "漫游时是否使用移动数据"}
                </FieldDescription>
                <FieldError errors={[errors.roaming]} />
              </FieldContent>
              <Controller
                control={form.control}
                name="roaming"
                render={({ field }) => (
                  <Switch
                    id="data-roaming"
                    checked={field.value}
                    aria-invalid={!!errors.roaming}
                    onCheckedChange={field.onChange}
                  />
                )}
              />
            </Field>
          </FieldGroup>
        </form>
      </CardContent>
      <CardFooter className="flex flex-wrap gap-2">
        <PendingButton
          type="submit"
          form="data-settings"
          pending={save.isPending}
          disabled={!isDirty}
        >
          保存
        </PendingButton>
        <Button
          variant="outline"
          disabled={!isDirty || save.isPending}
          onClick={() => form.reset(defaults(d))}
        >
          还原
        </Button>
      </CardFooter>
    </Card>
  );
}
