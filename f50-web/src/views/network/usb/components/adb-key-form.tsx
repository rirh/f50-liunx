import { useMemo, useState } from "react";
import { FormDialog } from "@/components/f50/form-dialog";
import { Button } from "@/components/ui/button";
import { Field, FieldError, FieldLabel } from "@/components/ui/field";
import { Textarea } from "@/components/ui/textarea";
import { t } from "@/i18n";
import { reportFormError } from "@/lib/form-error";
import { zodResolver } from "@hookform/resolvers/zod";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { KeyRoundIcon } from "lucide-react";
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

/**
 * Authorising a computer's adb key: a 700-character paste that would crowd the adb card, so it opens from the card's
 * header in a dialog.
 */
export function AdbKeyDialog() {
  const qc = useQueryClient();
  const [open, setOpen] = useState(false);
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
      setOpen(false);
      qc.invalidateQueries({ queryKey: usbKeys.info });
    },
    onError: (e) => reportFormError(e, form.setError, ["key"]),
  });
  return (
    <>
      <Button
        variant="outline"
        size="sm"
        onClick={() => {
          form.reset({ key: "" });
          setOpen(true);
        }}
      >
        <KeyRoundIcon data-icon="inline-start" />
        {t("network.usb.adbKey.add")}
      </Button>
      <FormDialog
        open={open}
        onOpenChange={setOpen}
        title={t("network.usb.adbKey.add")}
        description={t("network.usb.adbKey.hint")}
        formId="adb-key"
        pending={add.isPending}
        dirty={form.formState.isDirty}
        submitText={t("network.usb.adbKey.add")}
      >
        <form
          id="adb-key"
          onSubmit={form.handleSubmit((v) => add.mutate(v))}
          noValidate
        >
          <Controller
            control={form.control}
            name="key"
            render={({ field, fieldState }) => (
              <Field data-invalid={fieldState.invalid}>
                <FieldLabel htmlFor="adb-key-input">
                  {t("network.usb.adbKey.label")}
                </FieldLabel>
                <Textarea
                  id="adb-key-input"
                  rows={6}
                  spellCheck={false}
                  className="font-mono text-xs break-all"
                  placeholder="QAAAA…= user@host"
                  aria-invalid={fieldState.invalid}
                  {...field}
                />
                <FieldError errors={[fieldState.error]} />
              </Field>
            )}
          />
        </form>
      </FormDialog>
    </>
  );
}
