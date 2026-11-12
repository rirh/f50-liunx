import { PendingButton } from "@/components/f50/pending-button";
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert";
import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import {
  Field,
  FieldDescription,
  FieldError,
  FieldGroup,
  FieldLabel,
} from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { reportFormError } from "@/lib/form-error";
import { zodResolver } from "@hookform/resolvers/zod";
import { useMutation } from "@tanstack/react-query";
import { Controller, useForm, useWatch } from "react-hook-form";
import { z } from "zod";
import { SSID_HINT, pskOk, rawKeyOk, ssidOk } from "../../shared/text-rules";
import { joinNetwork } from "../api";
import type { JoinArgs } from "../types";

/** `client.rs join`: a name 1-32 bytes; a WPA passphrase or a raw key, required unless the network is open. */
const schema = z
  .object({
    ssid: z.string().refine(ssidOk, SSID_HINT),
    password: z.string(),
    open: z.boolean(),
  })
  .superRefine((v, ctx) => {
    if (v.open) return;
    if (!v.password)
      ctx.addIssue({
        code: "custom",
        path: ["password"],
        message: "请输入密码，或勾选开放网络",
      });
    else if (!pskOk(v.password) && !rawKeyOk(v.password)) {
      ctx.addIssue({
        code: "custom",
        path: ["password"],
        message: "8-63 个可打印 ASCII 字符，或 64 位十六进制密钥",
      });
    }
  });
type JoinForm = z.infer<typeof schema>;
const FIELDS = ["ssid", "password", "open"] as const;

export interface JoinTarget {
  ssid: string;
  open: boolean;
}

export function JoinDialog({
  target,
  hotspotOn,
  onOpenChange,
  onStarted,
}: {
  /** The network picked from the scan (empty name: typed by hand); `null`: closed. */
  target: JoinTarget | null;
  hotspotOn: boolean;
  onOpenChange: (open: boolean) => void;
  onStarted: (job: string, ssid: string) => void;
}) {
  return (
    <Dialog open={target !== null} onOpenChange={onOpenChange}>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>加入 Wi-Fi 网络</DialogTitle>
          <DialogDescription>
            作为上网线路连接，并保存为开机自动连接的网络。
          </DialogDescription>
        </DialogHeader>
        {target && (
          <JoinFormBody
            key={`${target.ssid}|${target.open}`}
            target={target}
            hotspotOn={hotspotOn}
            onCancel={() => onOpenChange(false)}
            onStarted={onStarted}
          />
        )}
      </DialogContent>
    </Dialog>
  );
}

function JoinFormBody({
  target,
  hotspotOn,
  onCancel,
  onStarted,
}: {
  target: JoinTarget;
  hotspotOn: boolean;
  onCancel: () => void;
  onStarted: (job: string, ssid: string) => void;
}) {
  const form = useForm<JoinForm>({
    resolver: zodResolver(schema),
    defaultValues: { ssid: target.ssid, password: "", open: target.open },
    mode: "onTouched",
  });
  const join = useMutation({
    mutationFn: joinNetwork,
    onSuccess: (r, args) => onStarted(r.job, args.ssid),
    onError: (e) => reportFormError(e, form.setError, FIELDS),
  });
  const open = useWatch({ control: form.control, name: "open" });

  const onSubmit = (v: JoinForm) => {
    const args: JoinArgs = v.open
      ? { ssid: v.ssid, open: true }
      : { ssid: v.ssid, password: v.password };
    join.mutate(args);
  };

  return (
    <form
      onSubmit={form.handleSubmit(onSubmit)}
      noValidate
      className="flex flex-col gap-4"
    >
      {hotspotOn && (
        <Alert>
          <AlertTitle>热点正在运行</AlertTitle>
          <AlertDescription>
            此驱动无法在不重启的情况下把热点切换为客户端：网络会被保存，重启后设备将先加入它（热点不再启动）。通过热点访问的浏览器会断开。
          </AlertDescription>
        </Alert>
      )}
      <FieldGroup>
        <Controller
          control={form.control}
          name="ssid"
          render={({ field, fieldState }) => (
            <Field data-invalid={fieldState.invalid}>
              <FieldLabel htmlFor="join-ssid">网络名称（SSID）</FieldLabel>
              <Input
                id="join-ssid"
                autoComplete="off"
                aria-invalid={fieldState.invalid}
                {...field}
              />
              <FieldError errors={[fieldState.error]} />
            </Field>
          )}
        />
        <Controller
          control={form.control}
          name="open"
          render={({ field }) => (
            <Field orientation="horizontal">
              <Checkbox
                id="join-open"
                checked={field.value}
                onCheckedChange={(c) => {
                  field.onChange(c === true);
                  if (c === true) form.clearErrors("password");
                }}
              />
              <FieldLabel htmlFor="join-open">开放网络（无密码）</FieldLabel>
            </Field>
          )}
        />
        <Controller
          control={form.control}
          name="password"
          render={({ field, fieldState }) => (
            <Field
              data-invalid={fieldState.invalid}
              data-disabled={open || undefined}
            >
              <FieldLabel htmlFor="join-password">密码</FieldLabel>
              <Input
                id="join-password"
                type="password"
                autoComplete="off"
                disabled={open}
                aria-invalid={fieldState.invalid}
                {...field}
              />
              {fieldState.error ? (
                <FieldError errors={[fieldState.error]} />
              ) : (
                <FieldDescription>
                  WPA2 密码（8-63 个字符）或 64 位十六进制密钥；不支持仅 WPA3
                  的网络
                </FieldDescription>
              )}
            </Field>
          )}
        />
      </FieldGroup>
      <DialogFooter>
        <Button
          type="button"
          variant="outline"
          onClick={onCancel}
          disabled={join.isPending}
        >
          取消
        </Button>
        <PendingButton type="submit" pending={join.isPending}>
          {join.isPending ? "提交中" : "加入"}
        </PendingButton>
      </DialogFooter>
    </form>
  );
}
