import { useState } from "react";
import { ResponsiveDialog } from "@/components/f50/form-dialog";
import { PendingButton } from "@/components/f50/pending-button";
import { Button } from "@/components/ui/button";
import { Field, FieldDescription, FieldLabel } from "@/components/ui/field";
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group";
import { t, type Key } from "@/i18n";
import { errorMessage } from "@/lib/format";
import { useMutation } from "@tanstack/react-query";
import { LogOutIcon } from "lucide-react";
import { toast } from "sonner";
import { leaveNetwork } from "../api";
import type { LeaveArgs } from "../types";

type SavedChoice = "disable" | "keep" | "forget";

const CHOICES: Record<SavedChoice, { label: Key; hint: Key; args: LeaveArgs }> =
  {
    disable: {
      label: "network.client.leave.disable.label",
      hint: "network.client.leave.disable.hint",
      args: {},
    },
    keep: {
      label: "network.client.leave.keep.label",
      hint: "network.client.leave.keep.hint",
      args: { keep: true },
    },
    forget: {
      label: "network.client.leave.forget.label",
      hint: "network.client.leave.forget.hint",
      args: { forget: true },
    },
  };

/**
 * Leave the network: the hotspot comes back; what happens to the saved network is the user's choice. One choice and
 * an action, not a settings form, so a plain responsive dialog (a drawer on a phone) with the action in its footer.
 */
export function LeaveDialog({
  disabled,
  onStarted,
}: {
  disabled?: boolean;
  onStarted: (job: string) => void;
}) {
  const [open, setOpen] = useState(false);
  const [choice, setChoice] = useState<SavedChoice>("disable");
  const leave = useMutation({
    mutationFn: leaveNetwork,
    onSuccess: (r) => {
      setOpen(false);
      onStarted(r.job);
    },
    onError: (e) => toast.error(errorMessage(e)),
  });
  return (
    <>
      <Button
        size="sm"
        variant="outline"
        disabled={disabled}
        onClick={() => {
          setChoice("disable");
          setOpen(true);
        }}
      >
        <LogOutIcon data-icon="inline-start" />
        {t("network.client.leave.action")}
      </Button>
      <ResponsiveDialog
        open={open}
        onOpenChange={(o) => !leave.isPending && setOpen(o)}
        title={t("network.client.leave.title")}
        description={t("network.client.leave.description")}
        footer={
          <>
            <Button
              variant="outline"
              onClick={() => setOpen(false)}
              disabled={leave.isPending}
            >
              {t("common.action.cancel")}
            </Button>
            <PendingButton
              variant="destructive"
              pending={leave.isPending}
              onClick={() => leave.mutate(CHOICES[choice].args)}
            >
              {leave.isPending
                ? t("network.shared.submitting")
                : t("network.client.leave.action")}
            </PendingButton>
          </>
        }
      >
        <Field>
          <FieldLabel>{t("network.client.leave.saved")}</FieldLabel>
          <ToggleGroup
            type="single"
            variant="outline"
            size="sm"
            value={choice}
            onValueChange={(v) => v && setChoice(v as SavedChoice)}
          >
            {(Object.keys(CHOICES) as SavedChoice[]).map((k) => (
              <ToggleGroupItem key={k} value={k}>
                {t(CHOICES[k].label)}
              </ToggleGroupItem>
            ))}
          </ToggleGroup>
          <FieldDescription>{t(CHOICES[choice].hint)}</FieldDescription>
        </Field>
      </ResponsiveDialog>
    </>
  );
}
