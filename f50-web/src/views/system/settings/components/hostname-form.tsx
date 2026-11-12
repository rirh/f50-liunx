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
import { reportFormError } from "@/lib/form-error";
import { zodResolver } from "@hookform/resolvers/zod";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { Controller, useForm } from "react-hook-form";
import { toast } from "sonner";
import { infoKey, setHostname } from "../api";
import { type HostnameValues, hostnameSchema } from "../schemas";

export function HostnameForm({ current }: { current: string }) {
  const qc = useQueryClient();
  const form = useForm<HostnameValues>({
    resolver: zodResolver(hostnameSchema),
    values: { name: current },
  });
  const save = useMutation({
    mutationFn: (v: HostnameValues) => setHostname(v.name),
    onSuccess: (r) => {
      toast.success(`主机名已改为 ${r.hostname}`);
      qc.invalidateQueries({ queryKey: infoKey });
    },
    onError: (e) => reportFormError(e, form.setError, { name: "name" }),
  });
  return (
    <Card>
      <CardHeader>
        <CardTitle>主机名</CardTitle>
        <CardDescription>设备在局域网中的名称</CardDescription>
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
                  <FieldLabel htmlFor="hostname">主机名</FieldLabel>
                  <Input
                    {...field}
                    id="hostname"
                    autoComplete="off"
                    spellCheck={false}
                    aria-invalid={fieldState.invalid}
                  />
                  <FieldDescription>
                    1–63 个字母、数字或连字符，不能以连字符开头或结尾。
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
            保存
          </PendingButton>
        </CardFooter>
      </form>
    </Card>
  );
}
