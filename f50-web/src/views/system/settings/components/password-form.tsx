import { useMemo, useState } from "react";
import { EditButton, FormDialog } from "@/components/f50/form-dialog";
import { Alert, AlertDescription } from "@/components/ui/alert";
import {
  Field,
  FieldError,
  FieldGroup,
  FieldLabel,
} from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { t } from "@/i18n";
import { reportFormError } from "@/lib/form-error";
import { zodResolver } from "@hookform/resolvers/zod";
import { useMutation } from "@tanstack/react-query";
import { Controller, useForm } from "react-hook-form";
import { toast } from "sonner";
import { SettingRow } from "./setting-row";
import { setPassword } from "../api";
import { type PasswordValues, passwordSchema } from "../schemas";

const EMPTY: PasswordValues = { password: "", confirm: "" };

/**
 * The root password as a row of the General card. The device never reveals it, so the row shows a mask; Change opens
 * an empty form in a dialog (cleared each time, so a typed password does not linger after Cancel).
 */
export function PasswordSetting() {
  const [open, setOpen] = useState(false);
  const schema = useMemo(() => passwordSchema(), []);
  const form = useForm<PasswordValues>({
    resolver: zodResolver(schema),
    defaultValues: EMPTY,
  });
  const save = useMutation({
    mutationFn: (v: PasswordValues) => setPassword(v.password),
    onSuccess: () => {
      form.reset(EMPTY);
      setOpen(false);
      toast.success(t("system.settings.password.saved"), {
        description: t("system.settings.password.savedDescription"),
      });
    },
    onError: (e) => reportFormError(e, form.setError, { password: "password" }),
  });
  return (
    <>
      <SettingRow
        label={t("system.settings.password.title")}
        value={<span aria-hidden>••••••••</span>}
        action={
          <EditButton
            onClick={() => {
              form.reset(EMPTY);
              setOpen(true);
            }}
          >
            {t("system.settings.password.change")}
          </EditButton>
        }
      />
      <FormDialog
        open={open}
        onOpenChange={setOpen}
        title={t("system.settings.password.title")}
        description={t("system.settings.password.description")}
        formId="password-form"
        size="sm"
        pending={save.isPending}
        dirty={form.formState.isDirty}
        submitText={t("system.settings.password.submit")}
      >
        <form
          id="password-form"
          onSubmit={form.handleSubmit((v) => save.mutate(v))}
          className="flex flex-col gap-4"
        >
          <FieldGroup>
            <Controller
              name="password"
              control={form.control}
              render={({ field, fieldState }) => (
                <Field data-invalid={fieldState.invalid}>
                  <FieldLabel htmlFor="new-password">
                    {t("system.settings.password.label")}
                  </FieldLabel>
                  <Input
                    {...field}
                    id="new-password"
                    type="password"
                    autoComplete="new-password"
                    aria-invalid={fieldState.invalid}
                  />
                  {fieldState.invalid && (
                    <FieldError errors={[fieldState.error]} />
                  )}
                </Field>
              )}
            />
            <Controller
              name="confirm"
              control={form.control}
              render={({ field, fieldState }) => (
                <Field data-invalid={fieldState.invalid}>
                  <FieldLabel htmlFor="confirm-password">
                    {t("system.settings.password.confirmLabel")}
                  </FieldLabel>
                  <Input
                    {...field}
                    id="confirm-password"
                    type="password"
                    autoComplete="new-password"
                    aria-invalid={fieldState.invalid}
                  />
                  {fieldState.invalid && (
                    <FieldError errors={[fieldState.error]} />
                  )}
                </Field>
              )}
            />
          </FieldGroup>
          <Alert>
            <AlertDescription>
              {t("system.settings.password.note")}
            </AlertDescription>
          </Alert>
        </form>
      </FormDialog>
    </>
  );
}
