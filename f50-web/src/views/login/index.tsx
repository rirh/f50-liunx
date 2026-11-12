import { Navigate, useLocation, useNavigate } from "react-router-dom";
import { Alert, AlertDescription } from "@/components/ui/alert";
import { Button } from "@/components/ui/button";
import {
  Card,
  CardContent,
  CardDescription,
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
import { SweepShine } from "@/components/ui/sweep-shine";
import { useSession } from "@/hooks/use-session";
import { GlobalChrome } from "@/layout";
import { errorMessage } from "@/lib/format";
import { login } from "@/lib/ubus";
import { zodResolver } from "@hookform/resolvers/zod";
import { useMutation } from "@tanstack/react-query";
import { RouterIcon } from "lucide-react";
import { useForm } from "react-hook-form";
import { z } from "zod";

const schema = z.object({
  username: z.string().min(1, "请输入用户名"),
  password: z.string(),
});
type Values = z.infer<typeof schema>;

export default function LoginPage() {
  const session = useSession();
  const navigate = useNavigate();
  const from = (useLocation().state as { from?: string } | null)?.from ?? "/";
  const form = useForm<Values>({
    resolver: zodResolver(schema),
    defaultValues: { username: "root", password: "" },
  });
  const signIn = useMutation({
    mutationFn: (v: Values) => login(v.username, v.password),
    onSuccess: () => navigate(from, { replace: true }),
  });

  if (session) return <Navigate to={from} replace />;

  return (
    <div className="flex min-h-dvh items-center justify-center bg-muted/50 p-4">
      <Card className="w-full max-w-sm">
        <CardHeader>
          <CardTitle className="flex items-center gap-2">
            <RouterIcon />
            ZTE F50
          </CardTitle>
          <CardDescription>
            使用设备的系统账户登录（OpenWrt 的 root 密码）
          </CardDescription>
        </CardHeader>
        <CardContent>
          <form onSubmit={form.handleSubmit((v) => signIn.mutate(v))}>
            <FieldGroup>
              {signIn.isError && (
                <Alert variant="destructive">
                  <AlertDescription>
                    {errorMessage(signIn.error)}
                  </AlertDescription>
                </Alert>
              )}
              <Field data-invalid={!!form.formState.errors.username}>
                <FieldLabel htmlFor="username">用户名</FieldLabel>
                <Input
                  id="username"
                  autoComplete="username"
                  aria-invalid={!!form.formState.errors.username}
                  {...form.register("username")}
                />
                <FieldError errors={[form.formState.errors.username]} />
              </Field>
              <Field>
                <FieldLabel htmlFor="password">密码</FieldLabel>
                <Input
                  id="password"
                  type="password"
                  autoComplete="current-password"
                  {...form.register("password")}
                />
              </Field>
              <Button
                type="submit"
                disabled={signIn.isPending}
                aria-busy={signIn.isPending}
              >
                {signIn.isPending ? <SweepShine>登录</SweepShine> : "登录"}
              </Button>
            </FieldGroup>
          </form>
        </CardContent>
      </Card>
      <GlobalChrome />
    </div>
  );
}
