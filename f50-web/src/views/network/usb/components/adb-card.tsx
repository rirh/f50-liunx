import { useState } from "react";
import { KvList } from "@/components/f50/kv-list";
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert";
import { Badge } from "@/components/ui/badge";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import {
  Field,
  FieldDescription,
  FieldGroup,
  FieldLabel,
  FieldSeparator,
} from "@/components/ui/field";
import { Switch } from "@/components/ui/switch";
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group";
import { errorMessage } from "@/lib/format";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { toast } from "sonner";
import { AdbKeyForm } from "./adb-key-form";
import { ConfirmDialog } from "../../shared/confirm-dialog";
import { setAdb, usbKeys } from "../api";
import { ADB_STATE_LABEL } from "../labels";
import type { AdbArgs, AdbSummary } from "../types";

/** adb over USB: on/off and whom it lets in, the authorised keys. */
export function UsbAdbCard({
  adb,
  hostRole,
}: {
  adb: AdbSummary;
  hostRole: boolean;
}) {
  const qc = useQueryClient();
  const [confirmOpen, setConfirmOpen] = useState(false);
  const mutation = useMutation({
    mutationFn: setAdb,
    onSuccess: (r) => {
      toast.success(
        r.restart ? "已保存，adb 正在重启（USB 连接会短暂中断）" : "已保存",
      );
      qc.invalidateQueries({ queryKey: usbKeys.info });
      if (r.restart)
        setTimeout(
          () => qc.invalidateQueries({ queryKey: usbKeys.info }),
          3000,
        );
    },
    onError: (e) => toast.error(errorMessage(e)),
  });
  const busy = mutation.isPending;
  const sending: AdbArgs | undefined = busy ? mutation.variables : undefined;

  return (
    <Card>
      <CardHeader>
        <CardTitle className="flex flex-wrap items-center gap-2">
          ADB 调试
          <Badge
            variant={
              adb.state === "open"
                ? "destructive"
                : adb.state === "keys"
                  ? "default"
                  : "secondary"
            }
          >
            {ADB_STATE_LABEL[adb.state]}
          </Badge>
        </CardTitle>
        <CardDescription>
          通过 USB 的 adb（root shell）。修改后 USB 连接会短暂中断。
        </CardDescription>
      </CardHeader>
      <CardContent className="flex flex-col gap-4">
        {adb.state === "open" && (
          <Alert variant="destructive">
            <AlertTitle>adb 处于开放模式</AlertTitle>
            <AlertDescription>
              任何插入 USB 口的电脑都能免密码获得 root shell。
            </AlertDescription>
          </Alert>
        )}
        {adb.damaged && (
          <Alert variant="destructive">
            <AlertTitle>adb 配置文件损坏</AlertTitle>
            <AlertDescription>
              /etc/mu300/adb 既不是 on 也不是 off，adb
              保持关闭。重新开关一次即可修复。
            </AlertDescription>
          </Alert>
        )}
        {adb.state === "keys" && adb.keys === 0 && (
          <Alert>
            <AlertTitle>尚未授权任何密钥</AlertTitle>
            <AlertDescription>
              仅授权密钥模式下没有密钥，任何电脑都无法连接。
            </AlertDescription>
          </Alert>
        )}
        {hostRole && adb.on && (
          <Alert>
            <AlertTitle>主机模式</AlertTitle>
            <AlertDescription>
              端口为主机模式时 adb 不可用，切回设备模式后恢复。
            </AlertDescription>
          </Alert>
        )}
        <KvList
          items={[
            { label: "已授权密钥", value: `${adb.keys} 个` },
            { label: "在 USB 功能中", value: adb.in_gadget ? "是" : "否" },
            { label: "守护进程", value: adb.running ? "运行中" : "未运行" },
            ...(adb.auth_reason
              ? [{ label: "授权说明", value: adb.auth_reason }]
              : []),
          ]}
        />
        <FieldGroup>
          <Field orientation="horizontal" data-disabled={busy || undefined}>
            <Switch
              id="adb-on"
              checked={sending?.state ? sending.state === "on" : adb.on}
              disabled={busy}
              aria-busy={sending?.state !== undefined}
              onCheckedChange={(on) =>
                mutation.mutate({ state: on ? "on" : "off" })
              }
            />
            <FieldLabel htmlFor="adb-on">启用 adb</FieldLabel>
          </Field>
          <Field data-disabled={busy || undefined}>
            <FieldLabel>访问控制</FieldLabel>
            <ToggleGroup
              type="single"
              variant="outline"
              size="sm"
              disabled={busy}
              aria-busy={sending?.auth !== undefined}
              value={sending?.auth ?? adb.auth}
              onValueChange={(v) => {
                if (v === "open") setConfirmOpen(true);
                else if (v === "keys") mutation.mutate({ auth: "keys" });
              }}
            >
              <ToggleGroupItem value="keys">仅授权密钥</ToggleGroupItem>
              <ToggleGroupItem value="open">开放</ToggleGroupItem>
            </ToggleGroup>
            <FieldDescription>
              {busy
                ? "正在保存…"
                : "推荐“仅授权密钥”，并在下方添加电脑的 adbkey.pub"}
            </FieldDescription>
          </Field>
          <FieldSeparator />
          <AdbKeyForm />
        </FieldGroup>
      </CardContent>
      <ConfirmDialog
        open={confirmOpen}
        onOpenChange={setConfirmOpen}
        title="改为开放模式？"
        confirmText="改为开放"
        destructive
        description={
          <span>
            开放后，任何插入 USB 口的电脑都能免密码获得设备的 root
            shell。仅在可信环境中临时使用。
          </span>
        }
        onConfirm={() => mutation.mutate({ auth: "open" })}
      />
    </Card>
  );
}
