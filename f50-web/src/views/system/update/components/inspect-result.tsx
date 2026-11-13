import { ConfirmButton } from "@/components/f50/confirm-button";
import { KvList } from "@/components/f50/kv-list";
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert";
import { Badge } from "@/components/ui/badge";
import { Checkbox } from "@/components/ui/checkbox";
import {
  Field,
  FieldError,
  FieldGroup,
  FieldLabel,
} from "@/components/ui/field";
import { joinList, t } from "@/i18n";
import { errorMessage, formatBytes } from "@/lib/format";
import { ApiError } from "@/lib/ubus";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { Controller, useForm } from "react-hook-form";
import { toast } from "sonner";
import { UPDATE_KEY, applyFile, uploadCancel } from "../api";
import { useStartJob } from "../hooks";
import { sigLabel } from "../labels";
import type { Inspected } from "../types";

interface FormValues {
  unsigned: boolean;
  downgrade: boolean;
}

const yesNo = (ok: boolean, yes: string, no: string) => (
  <Badge variant={ok ? "secondary" : "destructive"}>{ok ? yes : no}</Badge>
);

/** What `upload_done` found the file to be, and installing it (each thing wrong with it confirmed first). */
export function InspectResult({
  inspected: r,
  busy,
}: {
  inspected: Inspected;
  busy: boolean;
}) {
  const qc = useQueryClient();
  const started = useStartJob();
  const form = useForm<FormValues>({
    defaultValues: { unsigned: false, downgrade: false },
  });
  const errors = form.formState.errors;
  const sigRefused = r.signature === "bad" || r.signature === "badkey";
  const needsUnsigned = r.signature !== "valid" && !sigRefused;
  const usable = !r.refused && r.fits && r.whole && !sigRefused;

  const install = useMutation({
    mutationFn: (v: FormValues) => applyFile(r.sha256, v),
    onSuccess: started(
      r.tag
        ? t("maintenance.update.inspect.jobTitle", { tag: r.tag })
        : t("maintenance.update.jobTitle.applyFile"),
    ),
    onError: (e) => {
      if (
        e instanceof ApiError &&
        (e.field === "unsigned" || e.field === "downgrade")
      ) {
        form.setError(e.field, { message: errorMessage(e) });
      } else toast.error(errorMessage(e));
    },
  });
  const cancel = useMutation({
    mutationFn: uploadCancel,
    onSuccess: () => toast.success(t("maintenance.update.offline.deleted")),
    onError: (e) => toast.error(errorMessage(e)),
    onSettled: () => qc.invalidateQueries({ queryKey: UPDATE_KEY }),
  });

  const submit = form.handleSubmit((v) => {
    if (needsUnsigned && !v.unsigned)
      return form.setError("unsigned", {
        message: t("maintenance.update.inspect.unsignedRequired"),
      });
    if (r.downgrade && !v.downgrade)
      return form.setError("downgrade", {
        message: t("maintenance.update.inspect.downgradeRequired"),
      });
    install.mutate(v);
  });

  return (
    <div className="flex flex-col gap-3 rounded-lg border p-3">
      <KvList
        items={[
          {
            label: t("maintenance.update.inspect.release"),
            value: (
              <span className="tabular-nums">
                {r.tag ?? t("common.state.unknown")}
              </span>
            ),
          },
          {
            label: t("maintenance.update.inspect.size"),
            value: formatBytes(r.size),
          },
          {
            label: "SHA-256",
            value: (
              <span className="font-mono text-xs break-all">{r.sha256}</span>
            ),
          },
          {
            label: t("maintenance.update.inspect.signature"),
            value: (
              <Badge
                variant={r.signature === "valid" ? "secondary" : "destructive"}
              >
                {sigLabel(r.signature)}
              </Badge>
            ),
          },
          {
            label: t("maintenance.update.inspect.whole"),
            value: yesNo(
              r.whole,
              t("maintenance.update.inspect.wholeOk"),
              t("maintenance.update.inspect.wholeBad"),
            ),
          },
          {
            label: t("maintenance.update.inspect.fits"),
            value: r.fits
              ? t("maintenance.update.offer.willUpdate", {
                  systems: joinList(r.systems),
                })
              : yesNo(false, "", t("maintenance.update.inspect.notFit")),
          },
          {
            label: t("maintenance.update.inspect.kernel"),
            value: r.kernel ? t("common.state.yes") : t("common.state.no"),
          },
          {
            label: t("maintenance.update.inspect.downgrade"),
            value: r.downgrade ? (
              <Badge variant="destructive">
                {t("maintenance.update.inspect.older")}
              </Badge>
            ) : (
              t("common.state.no")
            ),
          },
        ]}
      />
      {r.refused && (
        <Alert variant="destructive">
          <AlertTitle>{t("maintenance.update.inspect.refused")}</AlertTitle>
          <AlertDescription>{r.refused}</AlertDescription>
        </Alert>
      )}
      {usable && (needsUnsigned || r.downgrade) && (
        <FieldGroup>
          {needsUnsigned && (
            <Controller
              control={form.control}
              name="unsigned"
              render={({ field }) => (
                <Field
                  orientation="horizontal"
                  data-invalid={!!errors.unsigned || undefined}
                >
                  <Checkbox
                    id="apply-unsigned"
                    checked={field.value}
                    onCheckedChange={(v) => field.onChange(v === true)}
                    aria-invalid={!!errors.unsigned || undefined}
                  />
                  <FieldLabel htmlFor="apply-unsigned">
                    {t("maintenance.update.inspect.confirmUnsigned")}
                  </FieldLabel>
                  {errors.unsigned && (
                    <FieldError>{errors.unsigned.message}</FieldError>
                  )}
                </Field>
              )}
            />
          )}
          {r.downgrade && (
            <Controller
              control={form.control}
              name="downgrade"
              render={({ field }) => (
                <Field
                  orientation="horizontal"
                  data-invalid={!!errors.downgrade || undefined}
                >
                  <Checkbox
                    id="apply-downgrade"
                    checked={field.value}
                    onCheckedChange={(v) => field.onChange(v === true)}
                    aria-invalid={!!errors.downgrade || undefined}
                  />
                  <FieldLabel htmlFor="apply-downgrade">
                    {t("maintenance.update.inspect.confirmDowngrade")}
                  </FieldLabel>
                  {errors.downgrade && (
                    <FieldError>{errors.downgrade.message}</FieldError>
                  )}
                </Field>
              )}
            />
          )}
        </FieldGroup>
      )}
      <div className="flex flex-wrap gap-2">
        <ConfirmButton
          size="sm"
          destructive={false}
          pending={install.isPending}
          disabled={busy || !usable || cancel.isPending}
          title={
            r.tag
              ? t("maintenance.update.offer.confirmTitle", { tag: r.tag })
              : t("maintenance.update.inspect.confirmTitle")
          }
          confirmText={t("maintenance.update.offer.confirmText")}
          description={
            <span className="flex flex-col gap-2">
              <span>
                {t(
                  r.kernel
                    ? "maintenance.update.inspect.willWriteKernel"
                    : "maintenance.update.inspect.willWrite",
                  { systems: joinList(r.systems) || "—" },
                )}
              </span>
              <span className="font-medium text-destructive">
                {t("maintenance.update.offer.noStop")}
              </span>
            </span>
          }
          onConfirm={() => void submit()}
        >
          {install.isPending
            ? t("maintenance.update.starting")
            : t("maintenance.update.inspect.install")}
        </ConfirmButton>
        <ConfirmButton
          size="sm"
          variant="outline"
          pending={cancel.isPending}
          disabled={busy || install.isPending}
          title={t("maintenance.update.offline.deleteTitle")}
          description={t("maintenance.update.inspect.deleteBody")}
          confirmText={t("common.action.delete")}
          onConfirm={() => cancel.mutate()}
        >
          {t("maintenance.update.inspect.delete")}
        </ConfirmButton>
      </div>
    </div>
  );
}
