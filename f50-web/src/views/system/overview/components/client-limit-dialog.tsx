import { FormDialog } from "@/components/f50/form-dialog";
import { Field, FieldLabel } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { t } from "@/i18n";
import { errorMessage } from "@/lib/format";
import { useMutation } from "@tanstack/react-query";
import { toast } from "sonner";
import { setClientLimit } from "../api";
import type { ClientLimit, ConnectedClient } from "../types";

const fields = [
  {
    key: "cap_bytes",
    label: "capField",
    multiplier: 1024 ** 2,
    max: Number.MAX_SAFE_INTEGER,
  },
  {
    key: "upload_kbps",
    label: "uploadField",
    multiplier: 1_000,
    max: 1_000_000,
  },
  {
    key: "download_kbps",
    label: "downloadField",
    multiplier: 1_000,
    max: 1_000_000,
  },
] as const;

export function ClientLimitDialog({
  client,
  limit,
  onClose,
  onSaved,
}: {
  client: ConnectedClient;
  limit?: ClientLimit;
  onClose: () => void;
  onSaved: () => void;
}) {
  const save = useMutation({
    mutationFn: setClientLimit,
    onSuccess: () => {
      onSaved();
      onClose();
    },
    onError: (e) => toast.error(errorMessage(e)),
  });
  return (
    <FormDialog
      open
      onOpenChange={(open) => !open && onClose()}
      title={t("system.overview.clients.limits.title")}
      description={t("system.overview.clients.limits.hint", {
        device: client.host || client.mac,
      })}
      size="sm"
      formId="client-limit-form"
      pending={save.isPending}
    >
      <form
        id="client-limit-form"
        className="flex flex-col gap-4"
        onSubmit={(e) => {
          e.preventDefault();
          const form = new FormData(e.currentTarget);
          const [cap_bytes, upload_kbps, download_kbps] = fields.map(
            ({ key, multiplier, max }) => {
              const value = Number(form.get(key));
              return Math.min(max, Math.round(value * multiplier));
            },
          );
          save.mutate({
            mac: client.mac,
            cap_bytes,
            upload_kbps,
            download_kbps,
          });
        }}
      >
        {fields.map(({ key, label, multiplier, max }) => (
          <Field key={key}>
            <FieldLabel htmlFor={key}>
              {t(`system.overview.clients.limits.${label}`)}
            </FieldLabel>
            <Input
              id={key}
              name={key}
              type="number"
              inputMode="decimal"
              min={0}
              max={max / multiplier}
              step={1 / multiplier}
              defaultValue={limit?.[key] ? limit[key] / multiplier : ""}
              placeholder={t("system.overview.clients.limits.unlimited")}
              disabled={save.isPending}
            />
          </Field>
        ))}
      </form>
    </FormDialog>
  );
}
