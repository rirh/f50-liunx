import { ConfirmButton } from "@/components/f50/confirm-button";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import { t } from "@/i18n";
import { errorMessage } from "@/lib/format";
import { useMutation } from "@tanstack/react-query";
import { PowerIcon, RotateCwIcon } from "lucide-react";
import { toast } from "sonner";
import { poweroff, reboot } from "../api";

export type PowerAction = "reboot" | "poweroff";

/** Reboot and power-off: both answer first and act two seconds later. */
export function PowerCard({
  onStarted,
}: {
  onStarted: (a: PowerAction) => void;
}) {
  const doReboot = useMutation({
    mutationFn: reboot,
    onSuccess: () => onStarted("reboot"),
    onError: (e) => toast.error(errorMessage(e)),
  });
  const doPoweroff = useMutation({
    mutationFn: poweroff,
    onSuccess: () => onStarted("poweroff"),
    onError: (e) => toast.error(errorMessage(e)),
  });
  const busy = doReboot.isPending || doPoweroff.isPending;
  return (
    <Card>
      <CardHeader>
        <CardTitle>{t("system.settings.power.title")}</CardTitle>
        <CardDescription>
          {t("system.settings.power.description")}
        </CardDescription>
      </CardHeader>
      <CardContent className="flex flex-wrap gap-2">
        <ConfirmButton
          title={t("system.settings.power.rebootTitle")}
          description={t("system.settings.power.rebootDescription")}
          confirmText={t("common.action.reboot")}
          pending={doReboot.isPending}
          disabled={busy}
          onConfirm={() => doReboot.mutate()}
        >
          <RotateCwIcon data-icon="inline-start" />
          {t("common.action.reboot")}
        </ConfirmButton>
        <ConfirmButton
          title={t("system.settings.power.poweroffTitle")}
          description={t("system.settings.power.poweroffDescription")}
          confirmText={t("system.settings.power.poweroff")}
          variant="outline"
          pending={doPoweroff.isPending}
          disabled={busy}
          onConfirm={() => doPoweroff.mutate()}
        >
          <PowerIcon data-icon="inline-start" />
          {t("system.settings.power.poweroff")}
        </ConfirmButton>
      </CardContent>
    </Card>
  );
}
