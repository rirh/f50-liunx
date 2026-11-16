import { useEffect, useMemo, useRef } from "react";
import { FormDialog, ResponsiveDialog } from "@/components/f50/form-dialog";
import { PendingButton } from "@/components/f50/pending-button";
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert";
import {
  Field,
  FieldContent,
  FieldDescription,
  FieldError,
  FieldGroup,
  FieldLabel,
} from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { Skeleton } from "@/components/ui/skeleton";
import { Switch } from "@/components/ui/switch";
import { t } from "@/i18n";
import { reportFormError } from "@/lib/form-error";
import { errorMessage, formatAgo, formatTime } from "@/lib/format";
import { zodResolver } from "@hookform/resolvers/zod";
import { CircleAlertIcon, CircleCheckIcon, SendIcon } from "lucide-react";
import { Controller, useForm } from "react-hook-form";
import { toast } from "sonner";
import { z } from "zod";
import { usePush, useSetPush, useTestPush } from "../queries";
import type { PushChange, PushSettings } from "../types";

const FORM_ID = "sms-push-form";
/** What f50d accepts (`f50.sms push_set`). */
const TOKEN = /^\d{1,20}:[A-Za-z0-9_-]{20,100}$/;
const CHAT_ID = /^(-?\d{1,20}|@[A-Za-z][A-Za-z0-9_]{3,31})$/;
const API =
  /^https?:\/\/[A-Za-z0-9.-]{1,253}(:\d{1,5})?(\/[A-Za-z0-9._~%/-]{0,200})?$/;

/** The bot's own id: the token's part before the colon (a chat id equal to it is the bot itself). */
const botId = (token: string) => token.split(":")[0] || null;

const pushSchema = (tokenSet: boolean, savedBot: string | null) =>
  z
    .object({
      enabled: z.boolean(),
      token: z
        .string()
        .trim()
        .refine((v) => v === "" || TOKEN.test(v), t("sms.push.tokenInvalid")),
      chat_id: z
        .string()
        .trim()
        .refine((v) => v === "" || CHAT_ID.test(v), t("sms.push.chatInvalid")),
      api: z
        .string()
        .trim()
        .refine((v) => v === "" || API.test(v), t("sms.push.apiInvalid")),
      proxy: z.boolean(),
    })
    .superRefine((v, ctx) => {
      if (!v.enabled) return;
      if (!v.token && !tokenSet)
        ctx.addIssue({
          code: "custom",
          path: ["token"],
          message: t("sms.push.tokenRequired"),
        });
      if (!v.chat_id)
        ctx.addIssue({
          code: "custom",
          path: ["chat_id"],
          message: t("sms.push.chatRequired"),
        });
    })
    .superRefine((v, ctx) => {
      // whether push is on or not: a test checks it too
      const bot = v.token ? botId(v.token) : savedBot;
      if (bot && v.chat_id === bot)
        ctx.addIssue({
          code: "custom",
          path: ["chat_id"],
          message: t("sms.push.chatIsBot"),
        });
    });
type PushValues = z.infer<ReturnType<typeof pushSchema>>;

const valuesOf = (s: PushSettings): PushValues => ({
  enabled: s.enabled,
  token: "",
  chat_id: s.chat_id,
  api: s.api === s.default_api ? "" : s.api,
  proxy: s.proxy ?? false,
});

/**
 * The form as a change: an empty token keeps the saved one; an empty API base is the default. `proxy` only when the
 * device knows it (an older f50d refuses an unknown field).
 */
const changeOf = (v: PushValues, s: PushSettings): PushChange => ({
  enabled: v.enabled,
  chat_id: v.chat_id,
  api: v.api,
  ...(s.proxy === undefined ? {} : { proxy: v.proxy }),
  ...(v.token ? { token: v.token } : {}),
});

/**
 * Telegram push: every received SMS goes to a chat through a bot. The token is write-only (the device returns only
 * a hint); Test sends with what the form holds, before saving.
 */
export function PushDialog({
  open,
  onOpenChange,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
}) {
  const push = usePush(open);
  if (!push.data)
    return (
      <ResponsiveDialog
        open={open}
        onOpenChange={onOpenChange}
        title={t("sms.push.title")}
        description={t("sms.push.description")}
      >
        {push.isError ? (
          <Alert variant="destructive">
            <CircleAlertIcon />
            <AlertTitle>{t("sms.push.loadFailed")}</AlertTitle>
            <AlertDescription>{errorMessage(push.error)}</AlertDescription>
          </Alert>
        ) : (
          <div className="flex flex-col gap-3">
            <Skeleton className="h-9 w-full" />
            <Skeleton className="h-9 w-full" />
            <Skeleton className="h-9 w-full" />
          </div>
        )}
      </ResponsiveDialog>
    );
  // keyed by the saved settings, so reopening (or a save) shows the device's values again
  return (
    <PushPanel
      key={`${JSON.stringify(valuesOf(push.data))}|${push.data.token_hint}`}
      open={open}
      onOpenChange={onOpenChange}
      settings={push.data}
    />
  );
}

function PushPanel({
  open,
  onOpenChange,
  settings,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  settings: PushSettings;
}) {
  const setPush = useSetPush();
  const testPush = useTestPush();
  const form = useForm<PushValues>({
    resolver: zodResolver(
      useMemo(
        () =>
          pushSchema(
            settings.token_set,
            settings.token_hint ? botId(settings.token_hint) : null,
          ),
        [settings.token_set, settings.token_hint],
      ),
    ),
    defaultValues: valuesOf(settings),
  });
  // every opening starts from the device's values (a Cancel drops what was typed, the token above all); the polls
  // while it is open refresh the last outcome only, never what is being typed
  const wasOpen = useRef(open);
  useEffect(() => {
    if (open && !wasOpen.current) form.reset(valuesOf(settings));
    wasOpen.current = open;
  }, [open, form, settings]);
  const busy = setPush.isPending || testPush.isPending;
  const fields = ["token", "chat_id", "api"] as const;

  const save = form.handleSubmit((v) =>
    setPush.mutate(changeOf(v, settings), {
      onSuccess: () => {
        toast.success(t("sms.push.saved"));
        onOpenChange(false);
      },
      onError: (err) => reportFormError(err, form.setError, fields),
    }),
  );
  // Test needs a token and a chat id whether push is on or not
  const test = form.handleSubmit((v) => {
    if (!v.token && !settings.token_set) {
      form.setError("token", { message: t("sms.push.tokenRequired") });
      return;
    }
    if (!v.chat_id) {
      form.setError("chat_id", { message: t("sms.push.chatRequired") });
      return;
    }
    // push_test takes the connection fields only: whether push is on does not matter to a test
    const { enabled: _, ...test } = changeOf(v, settings);
    testPush.mutate(test, {
      onSuccess: () => toast.success(t("sms.push.testSent")),
      onError: (err) => reportFormError(err, form.setError, fields),
    });
  });

  return (
    <FormDialog
      open={open}
      onOpenChange={onOpenChange}
      title={t("sms.push.title")}
      description={t("sms.push.description")}
      formId={FORM_ID}
      pending={setPush.isPending}
      dirty={form.formState.isDirty}
      extra={
        <PendingButton
          type="button"
          variant="outline"
          pending={testPush.isPending}
          disabled={setPush.isPending}
          onClick={() => void test()}
          icon={<SendIcon data-icon="inline-start" />}
        >
          {t("sms.push.test")}
        </PendingButton>
      }
    >
      <form id={FORM_ID} className="flex flex-col gap-4" onSubmit={save}>
        <LastOutcome settings={settings} />
        <FieldGroup>
          <Controller
            name="enabled"
            control={form.control}
            render={({ field }) => (
              <Field orientation="horizontal">
                <FieldContent>
                  <FieldLabel htmlFor="push-enabled">
                    {t("sms.push.enabled")}
                  </FieldLabel>
                  <FieldDescription>
                    {t("sms.push.enabledDescription")}
                  </FieldDescription>
                </FieldContent>
                <Switch
                  id="push-enabled"
                  checked={field.value}
                  onCheckedChange={field.onChange}
                  disabled={busy}
                />
              </Field>
            )}
          />
          <Controller
            name="token"
            control={form.control}
            render={({ field, fieldState }) => (
              <Field data-invalid={fieldState.invalid}>
                <FieldLabel htmlFor="push-token">
                  {t("sms.push.token")}
                </FieldLabel>
                <Input
                  {...field}
                  id="push-token"
                  type="password"
                  autoComplete="off"
                  spellCheck={false}
                  placeholder={
                    settings.token_set
                      ? t("sms.push.tokenKeep", {
                          hint: settings.token_hint ?? "",
                        })
                      : "123456789:AAH…"
                  }
                  className="font-mono text-base sm:text-sm"
                  aria-invalid={fieldState.invalid}
                  disabled={busy}
                />
                <FieldDescription>{t("sms.push.tokenHint")}</FieldDescription>
                {fieldState.invalid && (
                  <FieldError errors={[fieldState.error]} />
                )}
              </Field>
            )}
          />
          <Controller
            name="chat_id"
            control={form.control}
            render={({ field, fieldState }) => (
              <Field data-invalid={fieldState.invalid}>
                <FieldLabel htmlFor="push-chat">
                  {t("sms.push.chat")}
                </FieldLabel>
                <Input
                  {...field}
                  id="push-chat"
                  autoComplete="off"
                  spellCheck={false}
                  placeholder="123456789 / -1001234567890 / @channel"
                  className="font-mono text-base sm:text-sm"
                  aria-invalid={fieldState.invalid}
                  disabled={busy}
                />
                <FieldDescription>{t("sms.push.chatHint")}</FieldDescription>
                {fieldState.invalid && (
                  <FieldError errors={[fieldState.error]} />
                )}
              </Field>
            )}
          />
          <Controller
            name="api"
            control={form.control}
            render={({ field, fieldState }) => (
              <Field data-invalid={fieldState.invalid}>
                <FieldLabel htmlFor="push-api">{t("sms.push.api")}</FieldLabel>
                <Input
                  {...field}
                  id="push-api"
                  inputMode="url"
                  autoComplete="off"
                  spellCheck={false}
                  placeholder={settings.default_api}
                  className="font-mono text-base sm:text-sm"
                  aria-invalid={fieldState.invalid}
                  disabled={busy}
                />
                <FieldDescription>{t("sms.push.apiHint")}</FieldDescription>
                {fieldState.invalid && (
                  <FieldError errors={[fieldState.error]} />
                )}
              </Field>
            )}
          />
          {settings.proxy !== undefined && (
            <Controller
              name="proxy"
              control={form.control}
              render={({ field }) => (
                <Field orientation="horizontal">
                  <FieldContent>
                    <FieldLabel htmlFor="push-proxy">
                      {t("sms.push.proxy")}
                    </FieldLabel>
                    <FieldDescription>
                      {field.value && !settings.proxy_running
                        ? t("sms.push.proxyIdle")
                        : t("sms.push.proxyDescription")}
                    </FieldDescription>
                  </FieldContent>
                  <Switch
                    id="push-proxy"
                    checked={field.value}
                    onCheckedChange={field.onChange}
                    disabled={busy}
                  />
                </Field>
              )}
            />
          )}
        </FieldGroup>
      </form>
    </FormDialog>
  );
}

/** How the last push (or test) went, while push has been used at all. */
function LastOutcome({ settings }: { settings: PushSettings }) {
  const last = settings.last;
  if (!last) return null;
  return (
    <Alert variant={last.ok ? "default" : "destructive"}>
      {last.ok ? <CircleCheckIcon /> : <CircleAlertIcon />}
      <AlertTitle title={formatTime(last.at)}>
        {last.ok
          ? t("sms.push.lastOk", { ago: formatAgo(last.at) })
          : t("sms.push.lastFailed", { ago: formatAgo(last.at) })}
        {settings.queued > 0 &&
          ` · ${t("sms.push.queued", { count: settings.queued })}`}
      </AlertTitle>
      {last.error && <AlertDescription>{last.error}</AlertDescription>}
    </Alert>
  );
}
