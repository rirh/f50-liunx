import { useMemo } from "react";
import { PendingButton } from "@/components/f50/pending-button";
import {
  Field,
  FieldDescription,
  FieldError,
  FieldLabel,
} from "@/components/ui/field";
import { Textarea } from "@/components/ui/textarea";
import { t } from "@/i18n";
import { reportFormError } from "@/lib/form-error";
import { zodResolver } from "@hookform/resolvers/zod";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { Controller, useForm } from "react-hook-form";
import { toast } from "sonner";
import { z } from "zod";
import { isTextLine } from "../../shared/text-rules";
import { addAdbKey, usbKeys } from "../api";

/** `adbd/auth.rs key_ok`: 700 base64 characters, `QAAAA` first, `=` padded; then an optional `user@host`. */
function keyOk(line: string): boolean {
  const s = line.trim();
  const i = s.indexOf(" ");
  const k = i < 0 ? s : s.slice(0, i);
  return (
    k.length === 700 && k.startsWith("QAAAA") && /^[A-Za-z0-9+/]+=$/.test(k)
  );
}

const schema = () =>
  z.object({
    key: z
      .string()
      .min(1, t("network.usb.adbKey.required"))
      .max(800, t("network.usb.adbKey.tooLong"))
      .refine(
        (v) => isTextLine(v) && !v.startsWith("-"),
        t("network.usb.adbKey.oneLine"),
      )
      .refine(keyOk, t("network.usb.adbKey.invalid")),
  });
type KeyForm = z.infer<ReturnType<typeof schema>>;

export function AdbKeyForm() {
  const qc = useQueryClient();
  const resolverSchema = useMemo(() => schema(), []);
  const form = useForm<KeyForm>({
    resolver: zodResolver(resolverSchema),
    defaultValues: { key: "" },
    mode: "onTouched",
  });
  const add = useMutation({
    mutationFn: (v: KeyForm) => addAdbKey(v.key.trim()),
    onSuccess: (r) => {
      if (r.added)
        toast.success(t("network.usb.adbKey.added", { count: r.keys }));
      else toast.info(t("network.usb.adbKey.exists"));
      form.reset({ key: "" });
      qc.invalidateQueries({ queryKey: usbKeys.info });
    },
    onError: (e) => reportFormError(e, form.setError, ["key"]),
  });
  return (
    <form
      onSubmit={form.handleSubmit((v) => add.mutate(v))}
      noValidate
      className="flex flex-col gap-3"
    >
      <Controller
        control={form.control}
        name="key"
        render={({ field, fieldState }) => (
          <Field data-invalid={fieldState.invalid}>
            <FieldLabel htmlFor="adb-key">
              {t("network.usb.adbKey.label")}
            </FieldLabel>
            <Textarea
              id="adb-key"
              rows={4}
              spellCheck={false}
              className="font-mono text-xs break-all"
              placeholder="QAAAA…= user@host"
              aria-invalid={fieldState.invalid}
              {...field}
            />
            {fieldState.error ? (
              <FieldError errors={[fieldState.error]} />
            ) : (
              <FieldDescription>
                {t("network.usb.adbKey.hint")}
              </FieldDescription>
            )}
          </Field>
        )}
      />
      <PendingButton
        type="submit"
        size="sm"
        className="self-end"
        pending={add.isPending}
      >
        {add.isPending
          ? t("network.usb.adbKey.adding")
          : t("network.usb.adbKey.add")}
      </PendingButton>
    </form>
  );
}
