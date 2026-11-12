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
import { reportFormError } from "@/lib/form-error";
import { zodResolver } from "@hookform/resolvers/zod";
import { useMutation } from "@tanstack/react-query";
import { Controller, useForm } from "react-hook-form";
import { toast } from "sonner";
import { setPassword } from "../api";
import { type PasswordValues, passwordSchema } from "../schemas";

const EMPTY: PasswordValues = { password: "", confirm: "" };

export function PasswordForm() {
  const form = useForm<PasswordValues>({
    resolver: zodResolver(passwordSchema),
    defaultValues: EMPTY,
  });
  const save = useMutation({
    mutationFn: (v: PasswordValues) => setPassword(v.password),
    onSuccess: () => {
      form.reset(EMPTY);
      toast.success("root 密码已修改", {
        description: "当前登录可能随之失效，届时请用新密码重新登录。",
      });
    },
    onError: (e) => reportFormError(e, form.setError, { password: "password" }),
  });
  return (
    <Card>
      <CardHeader>
        <CardTitle>管理密码</CardTitle>
        <CardDescription>
          修改 root 用户的密码（网页登录与 SSH 共用）
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
                  <FieldLabel htmlFor="new-password">新密码</FieldLabel>
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
                  <FieldLabel htmlFor="confirm-password">确认新密码</FieldLabel>
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
              修改成功后，当前登录会话可能结束，需要用新密码重新登录。
            </AlertDescription>
          </Alert>
        </CardContent>
        <CardFooter>
          <PendingButton type="submit" pending={save.isPending}>
            修改密码
          </PendingButton>
        </CardFooter>
      </form>
    </Card>
  );
}
