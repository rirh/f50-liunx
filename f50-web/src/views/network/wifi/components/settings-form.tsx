import { useMemo, useState } from "react";
import { PendingButton } from "@/components/f50/pending-button";
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert";
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
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group";
import { reportFormError } from "@/lib/form-error";
import { zodResolver } from "@hookform/resolvers/zod";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { Controller, useForm, useWatch } from "react-hook-form";
import { toast } from "sonner";
import { ConfirmDialog } from "../../shared/confirm-dialog";
import { setWifi, wifiKeys } from "../api";
import { channelsFor, widthsFor } from "../channels";
import {
  WIFI_FIELDS,
  type WifiForm,
  wifiArgs,
  wifiDefaults,
  wifiSchema,
} from "../schema";
import type { WifiInfo, WifiSetArgs } from "../types";

const ARG_LABEL: Record<keyof WifiSetArgs, string> = {
  ssid: "名称",
  password: "密码",
  band: "频段",
  channel: "信道",
  width: "频宽",
  hidden: "隐藏",
  country: "国家",
};

function argText(k: keyof WifiSetArgs, v: unknown): string {
  if (k === "password") return "（新密码）";
  if (k === "hidden") return v ? "是" : "否";
  if (k === "band") return `${String(v)} GHz`;
  if (v === "auto") return "自动";
  if (k === "width") return `${String(v)} MHz`;
  return String(v);
}

export function WifiSettingsForm({ info }: { info: WifiInfo }) {
  const qc = useQueryClient();
  const initial = useMemo(() => wifiDefaults(info), [info]);
  const schema = useMemo(() => wifiSchema(initial), [initial]);
  const form = useForm<WifiForm>({
    resolver: zodResolver(schema),
    defaultValues: initial,
    mode: "onTouched",
  });
  const [pending, setPending] = useState<WifiSetArgs | null>(null);

  const save = useMutation({
    mutationFn: setWifi,
    onSuccess: () => {
      toast.success("已保存，Wi-Fi 将在约 2 秒后重新加载");
      form.reset({ ...form.getValues(), password: "" });
      qc.invalidateQueries({ queryKey: wifiKeys.info });
    },
    onError: (e) => reportFormError(e, form.setError, WIFI_FIELDS),
  });

  const [band, country, channel] = useWatch({
    control: form.control,
    name: ["band", "country", "channel"],
  });
  const channels = channelsFor(band, country);
  const channelOptions = ["auto", ...channels.map(String)];
  if (!channelOptions.includes(channel)) channelOptions.push(channel);
  const widths = widthsFor(band, country, channel);
  const saeNow = info.security === "wpa3" || info.security === "wpa2-wpa3";

  /** A channel or width the new band/country no longer offers goes back to automatic. */
  const fitChannel = (nextBand: WifiForm["band"], cc: string, ch: string) => {
    const opts = /^[A-Z]{2}$/.test(cc) ? channelsFor(nextBand, cc) : [];
    if (ch !== "auto" && !opts.includes(Number(ch))) {
      form.setValue("channel", "auto", { shouldDirty: true });
      ch = "auto";
    }
    if (!widthsFor(nextBand, cc, ch).includes(form.getValues("width"))) {
      form.setValue("width", "auto", { shouldDirty: true });
    }
  };

  const onSubmit = (v: WifiForm) => {
    const args = wifiArgs(v, initial);
    if (Object.keys(args).length === 0) {
      toast.info("没有需要保存的修改");
      return;
    }
    setPending(args);
  };

  return (
    <Card>
      <CardHeader>
        <CardTitle>热点设置</CardTitle>
        <CardDescription>
          只提交修改过的项；密码留空表示不修改。
        </CardDescription>
      </CardHeader>
      <form onSubmit={form.handleSubmit(onSubmit)} noValidate>
        <CardContent className="flex flex-col gap-4">
          {info.client_mode && (
            <Alert>
              <AlertTitle>无线电正用于 Wi-Fi 上网</AlertTitle>
              <AlertDescription>
                设置会先保存，热点恢复后生效。
              </AlertDescription>
            </Alert>
          )}
          {saeNow && (
            <Alert>
              <AlertTitle>此设备不支持 WPA3</AlertTitle>
              <AlertDescription>
                保存任何修改都会把热点改为
                WPA2-PSK（CCMP）；原密码不可用时需要输入新密码。
              </AlertDescription>
            </Alert>
          )}
          <FieldGroup className="grid gap-4 md:grid-cols-2">
            <Controller
              control={form.control}
              name="ssid"
              render={({ field, fieldState }) => (
                <Field data-invalid={fieldState.invalid}>
                  <FieldLabel htmlFor="wifi-ssid">网络名称（SSID）</FieldLabel>
                  <Input
                    id="wifi-ssid"
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
              name="password"
              render={({ field, fieldState }) => (
                <Field data-invalid={fieldState.invalid}>
                  <FieldLabel htmlFor="wifi-password">密码</FieldLabel>
                  <Input
                    id="wifi-password"
                    type="password"
                    autoComplete="new-password"
                    placeholder={
                      info.password_set
                        ? "已设置，留空不修改"
                        : "未设置（开放网络）"
                    }
                    aria-invalid={fieldState.invalid}
                    {...field}
                  />
                  {fieldState.error ? (
                    <FieldError errors={[fieldState.error]} />
                  ) : (
                    <FieldDescription>
                      8-63 个可打印 ASCII 字符
                      {info.security === "open"
                        ? "；开放热点设置密码后改为 WPA2"
                        : ""}
                    </FieldDescription>
                  )}
                </Field>
              )}
            />
            <Controller
              control={form.control}
              name="band"
              render={({ field, fieldState }) => (
                <Field data-invalid={fieldState.invalid}>
                  <FieldLabel>频段</FieldLabel>
                  <ToggleGroup
                    type="single"
                    variant="outline"
                    size="sm"
                    value={field.value}
                    aria-invalid={fieldState.invalid}
                    onValueChange={(v) => {
                      if (!v) return;
                      const b = v as WifiForm["band"];
                      field.onChange(b);
                      if (b !== initial.band) {
                        form.setValue("channel", "auto", { shouldDirty: true });
                        form.setValue("width", "auto", { shouldDirty: true });
                      } else {
                        fitChannel(
                          b,
                          form.getValues("country"),
                          form.getValues("channel"),
                        );
                      }
                    }}
                  >
                    <ToggleGroupItem value="2.4">2.4 GHz</ToggleGroupItem>
                    <ToggleGroupItem value="5">5 GHz</ToggleGroupItem>
                  </ToggleGroup>
                  <FieldError errors={[fieldState.error]} />
                </Field>
              )}
            />
            <Controller
              control={form.control}
              name="country"
              render={({ field, fieldState }) => (
                <Field data-invalid={fieldState.invalid}>
                  <FieldLabel htmlFor="wifi-country">国家/地区代码</FieldLabel>
                  <Input
                    id="wifi-country"
                    maxLength={2}
                    autoComplete="off"
                    className="uppercase"
                    aria-invalid={fieldState.invalid}
                    {...field}
                    onChange={(e) => {
                      const cc = e.target.value
                        .toUpperCase()
                        .replace(/[^A-Z]/g, "")
                        .slice(0, 2);
                      field.onChange(cc);
                      if (cc.length === 2)
                        fitChannel(
                          form.getValues("band"),
                          cc,
                          form.getValues("channel"),
                        );
                    }}
                  />
                  {fieldState.error ? (
                    <FieldError errors={[fieldState.error]} />
                  ) : (
                    <FieldDescription>
                      决定可用的 5 GHz 信道（{ap5Text(country)}）
                    </FieldDescription>
                  )}
                </Field>
              )}
            />
            <Controller
              control={form.control}
              name="channel"
              render={({ field, fieldState }) => (
                <Field data-invalid={fieldState.invalid}>
                  <FieldLabel htmlFor="wifi-channel">信道</FieldLabel>
                  <Select
                    value={field.value}
                    onValueChange={(v) => {
                      field.onChange(v);
                      if (
                        !widthsFor(
                          form.getValues("band"),
                          form.getValues("country"),
                          v,
                        ).includes(form.getValues("width"))
                      ) {
                        form.setValue("width", "auto", { shouldDirty: true });
                      }
                    }}
                  >
                    <SelectTrigger
                      id="wifi-channel"
                      className="w-full"
                      aria-invalid={fieldState.invalid}
                    >
                      <SelectValue />
                    </SelectTrigger>
                    <SelectContent>
                      <SelectGroup>
                        {channelOptions.map((c) => (
                          <SelectItem key={c} value={c}>
                            {c === "auto"
                              ? "自动"
                              : channels.includes(Number(c))
                                ? c
                                : `${c}（不可用）`}
                          </SelectItem>
                        ))}
                      </SelectGroup>
                    </SelectContent>
                  </Select>
                  {fieldState.error ? (
                    <FieldError errors={[fieldState.error]} />
                  ) : (
                    <FieldDescription>
                      不提供 DFS 信道（驱动不支持雷达检测）
                    </FieldDescription>
                  )}
                </Field>
              )}
            />
            <Controller
              control={form.control}
              name="width"
              render={({ field, fieldState }) => (
                <Field data-invalid={fieldState.invalid}>
                  <FieldLabel>频宽</FieldLabel>
                  <ToggleGroup
                    type="single"
                    variant="outline"
                    size="sm"
                    value={field.value}
                    aria-invalid={fieldState.invalid}
                    onValueChange={(v) => v && field.onChange(v)}
                  >
                    {["auto", "20", "40", "80"].map((w) => (
                      <ToggleGroupItem
                        key={w}
                        value={w}
                        disabled={!widths.includes(w)}
                      >
                        {w === "auto" ? "自动" : `${w} MHz`}
                      </ToggleGroupItem>
                    ))}
                  </ToggleGroup>
                  {fieldState.error ? (
                    <FieldError errors={[fieldState.error]} />
                  ) : (
                    <FieldDescription>
                      不超过所选信道在该国家允许的最大频宽
                    </FieldDescription>
                  )}
                </Field>
              )}
            />
            <Controller
              control={form.control}
              name="hidden"
              render={({ field }) => (
                <Field orientation="horizontal" className="md:col-span-2">
                  <Switch
                    id="wifi-hidden"
                    checked={field.value}
                    onCheckedChange={field.onChange}
                  />
                  <FieldLabel htmlFor="wifi-hidden">
                    隐藏网络名称（不广播 SSID）
                  </FieldLabel>
                </Field>
              )}
            />
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
        title="应用 Wi-Fi 设置？"
        confirmText="应用"
        description={
          <>
            <span>
              Wi-Fi 会在约 2
              秒后重新加载，所有热点客户端会短暂断开；如果你正通过热点访问控制台，可能需要用新设置重新连接。
            </span>
            {pending && (
              <span>
                {Object.entries(pending)
                  .map(
                    ([k, v]) =>
                      `${ARG_LABEL[k as keyof WifiSetArgs]}：${argText(k as keyof WifiSetArgs, v)}`,
                  )
                  .join("；")}
              </span>
            )}
          </>
        }
        onConfirm={() => pending && save.mutate(pending)}
      />
    </Card>
  );
}

function ap5Text(cc: string): string {
  if (!/^[A-Z]{2}$/.test(cc)) return "请输入两位代码";
  const l = channelsFor("5", cc);
  return l.length ? `5 GHz：${l.join(" ")}` : "无可用 5 GHz 信道";
}
