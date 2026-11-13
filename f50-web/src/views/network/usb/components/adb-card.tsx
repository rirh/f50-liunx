import { useState } from "react";
import { KvList } from "@/components/f50/kv-list";
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert";
import { Badge } from "@/components/ui/badge";
import {
  Card,
  CardAction,
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
} from "@/components/ui/field";
import { Switch } from "@/components/ui/switch";
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group";
import { t } from "@/i18n";
import { errorMessage } from "@/lib/format";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { toast } from "sonner";
import { AdbKeyDialog } from "./adb-key-form";
import { ConfirmDialog } from "../../shared/confirm-dialog";
import { setAdb, usbKeys } from "../api";
import { adbStateLabel } from "../labels";
import type { AdbArgs, AdbSummary } from "../types";

/** adb over USB: on/off and whom it lets in apply at once; adding an authorised key opens a dialog from the header. */
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
        r.restart
          ? t("network.usb.adb.savedRestart")
          : t("network.usb.adb.saved"),
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
          {t("network.usb.adb.title")}
          <Badge
            variant={
              adb.state === "open"
                ? "destructive"
                : adb.state === "keys"
                  ? "default"
                  : "secondary"
            }
          >
            {adbStateLabel(adb.state)}
          </Badge>
        </CardTitle>
        <CardDescription>{t("network.usb.adb.description")}</CardDescription>
        <CardAction>
          <AdbKeyDialog />
        </CardAction>
      </CardHeader>
      <CardContent className="flex flex-col gap-4">
        {adb.state === "open" && (
          <Alert variant="destructive">
            <AlertTitle>{t("network.usb.adb.openAlert.title")}</AlertTitle>
            <AlertDescription>
              {t("network.usb.adb.openAlert.description")}
            </AlertDescription>
          </Alert>
        )}
        {adb.damaged && (
          <Alert variant="destructive">
            <AlertTitle>{t("network.usb.adb.damaged.title")}</AlertTitle>
            <AlertDescription>
              {t("network.usb.adb.damaged.description")}
            </AlertDescription>
          </Alert>
        )}
        {adb.state === "keys" && adb.keys === 0 && (
          <Alert>
            <AlertTitle>{t("network.usb.adb.noKeys.title")}</AlertTitle>
            <AlertDescription>
              {t("network.usb.adb.noKeys.description")}
            </AlertDescription>
          </Alert>
        )}
        {hostRole && adb.on && (
          <Alert>
            <AlertTitle>{t("network.usb.adb.hostRole.title")}</AlertTitle>
            <AlertDescription>
              {t("network.usb.adb.hostRole.description")}
            </AlertDescription>
          </Alert>
        )}
        <KvList
          items={[
            {
              label: t("network.usb.adb.keys"),
              value: t("network.usb.adb.keyCount", { count: adb.keys }),
            },
            {
              label: t("network.usb.adb.inGadget"),
              value: adb.in_gadget
                ? t("common.state.yes")
                : t("common.state.no"),
            },
            {
              label: t("network.usb.adb.daemon"),
              value: adb.running
                ? t("common.state.running")
                : t("common.state.stopped"),
            },
            ...(adb.auth_reason
              ? [
                  {
                    label: t("network.usb.adb.authReason"),
                    value: adb.auth_reason,
                  },
                ]
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
            <FieldLabel htmlFor="adb-on">
              {t("network.usb.adb.enable")}
            </FieldLabel>
          </Field>
          <Field data-disabled={busy || undefined}>
            <FieldLabel>{t("network.usb.adb.access")}</FieldLabel>
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
              <ToggleGroupItem value="keys">
                {t("network.usb.adb.authKeys")}
              </ToggleGroupItem>
              <ToggleGroupItem value="open">
                {t("network.usb.adb.authOpen")}
              </ToggleGroupItem>
            </ToggleGroup>
            <FieldDescription>
              {busy
                ? t("network.usb.adb.savingHint")
                : t("network.usb.adb.accessHint")}
            </FieldDescription>
          </Field>
        </FieldGroup>
      </CardContent>
      <ConfirmDialog
        open={confirmOpen}
        onOpenChange={setConfirmOpen}
        title={t("network.usb.adb.confirmOpen.title")}
        confirmText={t("network.usb.adb.confirmOpen.confirm")}
        destructive
        description={
          <span>{t("network.usb.adb.confirmOpen.description")}</span>
        }
        onConfirm={() => mutation.mutate({ auth: "open" })}
      />
    </Card>
  );
}
