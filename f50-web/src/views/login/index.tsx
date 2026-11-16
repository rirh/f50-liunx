import { Navigate, useLocation, useNavigate } from "react-router-dom";
import { BrandMark } from "@/components/f50/brand-mark";
import { LanguageSwitcher } from "@/components/f50/language-switcher";
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
import { type Key, t } from "@/i18n";
import { GlobalChrome } from "@/layout";
import { errorMessage } from "@/lib/format";
import { login } from "@/lib/ubus";
import { zodResolver } from "@hookform/resolvers/zod";
import { useMutation } from "@tanstack/react-query";
import {
  GlobeLockIcon,
  type LucideIcon,
  ShuffleIcon,
  SignalIcon,
  SquareTerminalIcon,
} from "lucide-react";
import { useForm } from "react-hook-form";
import { z } from "zod";
import { useBackdrop } from "./backdrop";

/** Built at render: the message is in the current language. */
const schema = () =>
  z.object({
    username: z.string().min(1, t("app.login.usernameRequired")),
    password: z.string(),
  });
type Values = z.infer<ReturnType<typeof schema>>;

/** What the console does, beside the form on a wide screen. */
const FEATURES: { icon: LucideIcon; key: Key }[] = [
  { icon: SignalIcon, key: "app.login.feature.cellular" },
  { icon: GlobeLockIcon, key: "app.login.feature.proxy" },
  { icon: SquareTerminalIcon, key: "app.login.feature.shell" },
];

export default function LoginPage() {
  const session = useSession();
  const { backdrop, loading, shuffle } = useBackdrop();
  const navigate = useNavigate();
  const from = (useLocation().state as { from?: string } | null)?.from ?? "/";
  const form = useForm<Values>({
    resolver: zodResolver(schema()),
    defaultValues: { username: "root", password: "" },
  });
  const signIn = useMutation({
    mutationFn: (v: Values) => login(v.username, v.password),
    onSuccess: () => navigate(from, { replace: true }),
  });

  if (session) return <Navigate to={from} replace />;

  return (
    <div className="relative isolate flex min-h-dvh overflow-hidden bg-[#0b1417]">
      {/* the page's own backdrop: what shows while the photo loads, and without internet */}
      <div
        aria-hidden
        className="absolute inset-0 -z-30 bg-[radial-gradient(ellipse_at_15%_10%,rgba(45,212,191,0.32),transparent_55%),radial-gradient(ellipse_at_90%_95%,rgba(56,189,248,0.22),transparent_50%)]"
      />
      {backdrop && (
        <img
          key={backdrop.src}
          src={backdrop.src}
          alt=""
          aria-hidden
          className="absolute inset-0 -z-20 size-full animate-in object-cover duration-1000 fade-in motion-safe:zoom-in-105"
        />
      )}
      {/* a scrim, so white text and the card read on any photo */}
      <div
        aria-hidden
        className="absolute inset-0 -z-10 bg-linear-to-t from-black/75 via-black/35 to-black/45 lg:bg-linear-to-r lg:from-black/70 lg:via-black/35 lg:to-black/15"
      />

      <main className="mx-auto flex w-full max-w-6xl flex-col items-center justify-center gap-8 px-5 py-12 lg:flex-row lg:justify-between lg:gap-16 lg:px-12">
        <section className="flex w-full max-w-sm flex-col items-center gap-4 text-center text-white lg:max-w-lg lg:items-start lg:text-start">
          <BrandMark className="size-14 shadow-lg ring-1 ring-white/25 lg:size-16" />
          <div className="space-y-2">
            <h1 className="text-3xl font-semibold tracking-tight lg:text-5xl">
              {t("app.brand")}
            </h1>
            <p className="text-base text-white/80 lg:text-lg">
              {t("app.login.tagline")}
            </p>
          </div>
          <ul className="mt-4 hidden flex-col gap-3 text-sm text-white/85 lg:flex">
            {FEATURES.map(({ icon: Icon, key }) => (
              <li key={key} className="flex items-center gap-3">
                <span className="flex size-8 items-center justify-center rounded-lg bg-white/10 ring-1 ring-white/15 backdrop-blur">
                  <Icon className="size-4" />
                </span>
                {t(key)}
              </li>
            ))}
          </ul>
        </section>

        <Card className="w-full max-w-sm border-white/20 bg-background/85 shadow-2xl backdrop-blur-xl supports-[backdrop-filter]:bg-background/80 dark:border-white/10">
          <CardHeader>
            <CardTitle className="flex items-center gap-2 text-xl">
              {t("app.login.title")}
              <span className="ms-auto">
                <LanguageSwitcher />
              </span>
            </CardTitle>
            <CardDescription>{t("app.login.description")}</CardDescription>
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
                  <FieldLabel htmlFor="username">
                    {t("app.login.username")}
                  </FieldLabel>
                  <Input
                    id="username"
                    autoComplete="username"
                    aria-invalid={!!form.formState.errors.username}
                    {...form.register("username")}
                  />
                  <FieldError errors={[form.formState.errors.username]} />
                </Field>
                <Field>
                  <FieldLabel htmlFor="password">
                    {t("app.login.password")}
                  </FieldLabel>
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
                  {signIn.isPending ? (
                    <SweepShine>{t("app.login.submit")}</SweepShine>
                  ) : (
                    t("app.login.submit")
                  )}
                </Button>
              </FieldGroup>
            </form>
          </CardContent>
        </Card>
      </main>

      <footer className="absolute bottom-[max(0.75rem,env(safe-area-inset-bottom))] left-4 flex items-center gap-2 text-xs text-white/70">
        <button
          type="button"
          onClick={shuffle}
          disabled={loading}
          aria-label={t("app.login.shuffle")}
          title={t("app.login.shuffle")}
          className="flex size-7 items-center justify-center rounded-full bg-black/25 ring-1 ring-white/15 backdrop-blur transition-colors hover:bg-black/40 hover:text-white disabled:opacity-50"
        >
          <ShuffleIcon className="size-3.5" />
        </button>
        {loading ? (
          // while a photo loads, the credit's place says so with the shine the sign-in button uses
          <SweepShine>{t("app.login.photoLoading")}</SweepShine>
        ) : (
          backdrop?.author &&
          (backdrop.link ? (
            <a
              href={backdrop.link}
              target="_blank"
              rel="noreferrer noopener"
              className="hover:text-white hover:underline"
            >
              {t("app.login.photo", { author: backdrop.author })}
            </a>
          ) : (
            <span>{t("app.login.photo", { author: backdrop.author })}</span>
          ))
        )}
      </footer>
      <GlobalChrome />
    </div>
  );
}
