import { useMemo } from "react";
import { PendingButton } from "@/components/f50/pending-button";
import { Alert, AlertDescription } from "@/components/ui/alert";
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
import { setPassword } from "../api";
import { type PasswordValues, passwordSchema } from "../schemas";

const EMPTY: PasswordValues = { password: "", confirm: "" };

export function PasswordForm() {
  const schema = useMemo(() => passwordSchema(), []);
  const form = useForm<PasswordValues>({
    resolver: zodResolver(schema),
    defaultValues: EMPTY,
  });
  const save = useMutation({
    mutationFn: (v: PasswordValues) => setPassword(v.password),
    onSuccess: () => {
      form.reset(EMPTY);
      toast.success(t("system.settings.password.saved"), {
        description: t("system.settings.password.savedDescription"),
      });
    },
    onError: (e) => reportFormError(e, form.setError, { password: "password" }),
  });
  return (
    <Card>
      <CardHeader>
        <CardTitle>{t("system.settings.password.title")}</CardTitle>
        <CardDescription>
          {t("system.settings.password.description")}
        </CardDescription>
      </CardHeader>
      <form
        onSubmit={form.handleSubmit((v) => save.mutate(v))}
        className="contents"
      >
        <CardContent className="flex flex-col gap-4">
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
        </CardContent>
        <CardFooter>
          <PendingButton type="submit" pending={save.isPending}>
            {t("system.settings.password.submit")}
          </PendingButton>
        </CardFooter>
      </form>
    </Card>
  );
}
