import { useState } from "react";
import { FormDialog, ResponsiveDialog } from "@/components/f50/form-dialog";
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert";
import {
  Field,
  FieldContent,
  FieldDescription,
  FieldGroup,
  FieldLabel,
} from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { Skeleton } from "@/components/ui/skeleton";
import { Switch } from "@/components/ui/switch";
import { t } from "@/i18n";
import { errorMessage, formatAgo, formatTime } from "@/lib/format";
import { cn } from "@/lib/utils";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { CircleAlertIcon } from "lucide-react";
import { toast } from "sonner";
import { getRemote, setRemote } from "../api";
import type { RemoteSettings } from "../types";

const KEY = ["f50.remote"] as const;
const FORM_ID = "sms-remote-form";

/**
 * 短信遥控: commands by SMS from the allowed numbers (status, traffic, balance, the proxy, Wi-Fi, reboot), answered by
 * SMS. The secret is write-only (empty keeps it); the last commands are listed with their answers.
 */
export function RemoteDialog({
  open,
  onOpenChange,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
}) {
  const q = useQuery({
    queryKey: KEY,
    queryFn: getRemote,
    enabled: open,
    refetchInterval: open ? 10_000 : false,
  });
  if (!q.data)
    return (
      <ResponsiveDialog
        open={open}
        onOpenChange={onOpenChange}
        title={t("sms.remote.title")}
        description={t("sms.remote.description")}
      >
        {q.isError ? (
          <Alert variant="destructive">
            <CircleAlertIcon />
            <AlertTitle>{t("sms.remote.loadFailed")}</AlertTitle>
            <AlertDescription>{errorMessage(q.error)}</AlertDescription>
          </Alert>
        ) : (
          <div className="flex flex-col gap-3">
            <Skeleton className="h-9 w-full" />
            <Skeleton className="h-9 w-full" />
          </div>
        )}
      </ResponsiveDialog>
    );
  return (
    <RemotePanel
      key={`${q.data.enabled}|${q.data.numbers.join(",")}|${q.data.secret_set}`}
      open={open}
      onOpenChange={onOpenChange}
      s={q.data}
    />
  );
}

function RemotePanel({
  open,
  onOpenChange,
  s,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  s: RemoteSettings;
}) {
  const qc = useQueryClient();
  const [enabled, setEnabled] = useState(s.enabled);
  const [numbers, setNumbers] = useState(s.numbers.join(", "));
  const [secret, setSecret] = useState("");
  const [error, setError] = useState<string | null>(null);
  const save = useMutation({
    mutationFn: setRemote,
    onSuccess: (v) => {
      qc.setQueryData(KEY, v);
      toast.success(t("sms.remote.saved"));
      onOpenChange(false);
    },
    onError: (e) => setError(errorMessage(e)),
  });
  const dirty =
    enabled !== s.enabled || numbers !== s.numbers.join(", ") || !!secret;

  return (
    <FormDialog
      open={open}
      onOpenChange={onOpenChange}
      title={t("sms.remote.title")}
      description={t("sms.remote.description")}
      formId={FORM_ID}
      pending={save.isPending}
      dirty={dirty}
    >
      <form
        id={FORM_ID}
        className="flex flex-col gap-4"
        onSubmit={(e) => {
          e.preventDefault();
          setError(null);
          save.mutate({
            enabled,
            numbers,
            ...(secret ? { secret } : {}),
          });
        }}
      >
        <FieldGroup>
          <Field orientation="horizontal">
            <FieldContent>
              <FieldLabel htmlFor="remote-enabled">
                {t("sms.remote.enabled")}
              </FieldLabel>
              <FieldDescription>{t("sms.remote.enabledHint")}</FieldDescription>
            </FieldContent>
            <Switch
              id="remote-enabled"
              checked={enabled}
              onCheckedChange={setEnabled}
              disabled={save.isPending}
            />
          </Field>
          <Field data-invalid={!!error || undefined}>
            <FieldLabel htmlFor="remote-numbers">
              {t("sms.remote.numbers")}
            </FieldLabel>
            <Input
              id="remote-numbers"
              value={numbers}
              onChange={(e) => setNumbers(e.target.value)}
              placeholder="13800138000, 13900139000"
              inputMode="tel"
              autoComplete="off"
              className="font-mono text-base sm:text-sm"
              disabled={save.isPending}
            />
            <FieldDescription>{t("sms.remote.numbersHint")}</FieldDescription>
          </Field>
          <Field>
            <FieldLabel htmlFor="remote-secret">
              {t("sms.remote.secret")}
            </FieldLabel>
            <Input
              id="remote-secret"
              type="password"
              value={secret}
              onChange={(e) => setSecret(e.target.value)}
              placeholder={
                s.secret_set
                  ? t("sms.remote.secretKeep")
                  : t("sms.remote.secretNone")
              }
              autoComplete="off"
              className="text-base sm:text-sm"
              disabled={save.isPending}
            />
            <FieldDescription>{t("sms.remote.secretHint")}</FieldDescription>
          </Field>
        </FieldGroup>
        {error && (
          <Alert variant="destructive">
            <CircleAlertIcon />
            <AlertDescription>{error}</AlertDescription>
          </Alert>
        )}
        <div className="rounded-lg bg-muted/60 px-3 py-2 text-xs leading-5 text-muted-foreground">
          <p className="font-medium text-foreground">
            {t("sms.remote.commands")}
          </p>
          <p>{t("sms.remote.commandList")}</p>
          <p>
            {s.secret_set || secret
              ? t("sms.remote.exampleSecret")
              : t("sms.remote.example")}
          </p>
        </div>
        {s.history.length > 0 && (
          <div className="flex flex-col gap-1.5">
            <p className="text-xs font-medium text-muted-foreground">
              {t("sms.remote.history")}
            </p>
            <ul className="flex max-h-48 flex-col gap-1 overflow-y-auto">
              {s.history.map((h) => (
                <li
                  key={`${h.at}-${h.from}-${h.command}`}
                  className="rounded-md bg-muted/40 px-2.5 py-1.5 text-xs"
                >
                  <span className="flex items-center justify-between gap-2">
                    <span className="truncate font-medium">
                      <span
                        className={cn(
                          "me-1.5 inline-block size-1.5 rounded-full align-middle",
                          h.ok ? "bg-emerald-500" : "bg-destructive",
                        )}
                      />
                      {h.command}
                    </span>
                    <span
                      className="shrink-0 text-muted-foreground"
                      title={formatTime(h.at)}
                    >
                      {h.from} · {formatAgo(h.at)}
                    </span>
                  </span>
                  <span className="line-clamp-2 whitespace-pre-line text-muted-foreground">
                    {h.reply}
                  </span>
                </li>
              ))}
            </ul>
          </div>
        )}
      </form>
    </FormDialog>
  );
}
