import { useMemo } from "react";
import { PendingButton } from "@/components/f50/pending-button";
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
  FieldDescription,
  FieldError,
  FieldGroup,
  FieldLabel,
} from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { t } from "@/i18n";
import { reportFormError } from "@/lib/form-error";
import { zodResolver } from "@hookform/resolvers/zod";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { Controller, useForm } from "react-hook-form";
import { toast } from "sonner";
import { infoKey, setHostname } from "../api";
import { type HostnameValues, hostnameSchema } from "../schemas";

export function HostnameForm({ current }: { current: string }) {
  const qc = useQueryClient();
  const schema = useMemo(() => hostnameSchema(), []);
  const form = useForm<HostnameValues>({
    resolver: zodResolver(schema),
    values: { name: current },
  });
  const save = useMutation({
    mutationFn: (v: HostnameValues) => setHostname(v.name),
    onSuccess: (r) => {
      toast.success(t("system.settings.hostname.saved", { name: r.hostname }));
      qc.invalidateQueries({ queryKey: infoKey });
    },
    onError: (e) => reportFormError(e, form.setError, { name: "name" }),
  });
  return (
    <Card>
      <CardHeader>
        <CardTitle>{t("system.settings.hostname.title")}</CardTitle>
        <CardDescription>
          {t("system.settings.hostname.description")}
        </CardDescription>
      </CardHeader>
      <form
        onSubmit={form.handleSubmit((v) => save.mutate(v))}
        className="contents"
      >
        <CardContent>
          <FieldGroup>
            <Controller
              name="name"
              control={form.control}
              render={({ field, fieldState }) => (
                <Field data-invalid={fieldState.invalid}>
                  <FieldLabel htmlFor="hostname">
                    {t("system.settings.hostname.label")}
                  </FieldLabel>
                  <Input
                    {...field}
                    id="hostname"
                    autoComplete="off"
                    spellCheck={false}
                    aria-invalid={fieldState.invalid}
                  />
                  <FieldDescription>
                    {t("system.settings.hostname.hint")}
                  </FieldDescription>
                  {fieldState.invalid && (
                    <FieldError errors={[fieldState.error]} />
                  )}
                </Field>
              )}
            />
          </FieldGroup>
        </CardContent>
        <CardFooter>
          <PendingButton
            type="submit"
            pending={save.isPending}
            disabled={!form.formState.isDirty}
          >
            {t("common.action.save")}
          </PendingButton>
        </CardFooter>
      </form>
    </Card>
  );
}
