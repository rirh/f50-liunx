import { useMemo } from "react";
import { PendingButton } from "@/components/f50/pending-button";
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert";
import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
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
import { useMutation } from "@tanstack/react-query";
import { Controller, useForm, useWatch } from "react-hook-form";
import { z } from "zod";
import { ssidHint, pskOk, rawKeyOk, ssidOk } from "../../shared/text-rules";
import { joinNetwork } from "../api";
import type { JoinArgs } from "../types";

/** `client.rs join`: a name 1-32 bytes; a WPA passphrase or a raw key, required unless the network is open. */
const schema = () =>
  z
    .object({
      ssid: z.string().refine(ssidOk, ssidHint()),
      password: z.string(),
      open: z.boolean(),
    })
    .superRefine((v, ctx) => {
      if (v.open) return;
      if (!v.password)
        ctx.addIssue({
          code: "custom",
          path: ["password"],
          message: t("network.client.join.passwordRequired"),
        });
      else if (!pskOk(v.password) && !rawKeyOk(v.password)) {
        ctx.addIssue({
          code: "custom",
          path: ["password"],
          message: t("network.client.join.passwordInvalid"),
        });
      }
    });
type JoinForm = z.infer<ReturnType<typeof schema>>;
const FIELDS = ["ssid", "password", "open"] as const;

export interface JoinTarget {
  ssid: string;
  open: boolean;
}

export function JoinDialog({
  target,
  hotspotOn,
  onOpenChange,
  onStarted,
}: {
  /** The network picked from the scan (empty name: typed by hand); `null`: closed. */
  target: JoinTarget | null;
  hotspotOn: boolean;
  onOpenChange: (open: boolean) => void;
  onStarted: (job: string, ssid: string) => void;
}) {
  return (
    <Dialog open={target !== null} onOpenChange={onOpenChange}>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>{t("network.client.join.title")}</DialogTitle>
          <DialogDescription>
            {t("network.client.join.description")}
          </DialogDescription>
        </DialogHeader>
        {target && (
          <JoinFormBody
            key={`${target.ssid}|${target.open}`}
            target={target}
            hotspotOn={hotspotOn}
            onCancel={() => onOpenChange(false)}
            onStarted={onStarted}
          />
        )}
      </DialogContent>
    </Dialog>
  );
}

function JoinFormBody({
  target,
  hotspotOn,
  onCancel,
  onStarted,
}: {
  target: JoinTarget;
  hotspotOn: boolean;
  onCancel: () => void;
  onStarted: (job: string, ssid: string) => void;
}) {
  const resolverSchema = useMemo(() => schema(), []);
  const form = useForm<JoinForm>({
    resolver: zodResolver(resolverSchema),
    defaultValues: { ssid: target.ssid, password: "", open: target.open },
    mode: "onTouched",
  });
  const join = useMutation({
    mutationFn: joinNetwork,
    onSuccess: (r, args) => onStarted(r.job, args.ssid),
    onError: (e) => reportFormError(e, form.setError, FIELDS),
  });
  const open = useWatch({ control: form.control, name: "open" });

  const onSubmit = (v: JoinForm) => {
    const args: JoinArgs = v.open
      ? { ssid: v.ssid, open: true }
      : { ssid: v.ssid, password: v.password };
    join.mutate(args);
  };

  return (
    <form
      onSubmit={form.handleSubmit(onSubmit)}
      noValidate
      className="flex flex-col gap-4"
    >
      {hotspotOn && (
        <Alert>
          <AlertTitle>{t("network.client.join.hotspotOn.title")}</AlertTitle>
          <AlertDescription>
            {t("network.client.join.hotspotOn.description")}
          </AlertDescription>
        </Alert>
      )}
      <FieldGroup>
        <Controller
          control={form.control}
          name="ssid"
          render={({ field, fieldState }) => (
            <Field data-invalid={fieldState.invalid}>
              <FieldLabel htmlFor="join-ssid">
                {t("network.client.join.ssid")}
              </FieldLabel>
              <Input
                id="join-ssid"
                autoComplete="off"
                aria-invalid={fieldState.invalid}
                {...field}
              />
              <FieldError errors={[fieldState.error]} />
            </Field>
          )}
        />
        <Controller
          control={form.control}
          name="open"
          render={({ field }) => (
            <Field orientation="horizontal">
              <Checkbox
                id="join-open"
                checked={field.value}
                onCheckedChange={(c) => {
                  field.onChange(c === true);
                  if (c === true) form.clearErrors("password");
                }}
              />
              <FieldLabel htmlFor="join-open">
                {t("network.client.join.open")}
              </FieldLabel>
            </Field>
          )}
        />
        <Controller
          control={form.control}
          name="password"
          render={({ field, fieldState }) => (
            <Field
              data-invalid={fieldState.invalid}
              data-disabled={open || undefined}
            >
              <FieldLabel htmlFor="join-password">
                {t("network.client.join.password")}
              </FieldLabel>
              <Input
                id="join-password"
                type="password"
                autoComplete="off"
                disabled={open}
                aria-invalid={fieldState.invalid}
                {...field}
              />
              {fieldState.error ? (
                <FieldError errors={[fieldState.error]} />
              ) : (
                <FieldDescription>
                  {t("network.client.join.passwordHint")}
                </FieldDescription>
              )}
            </Field>
          )}
        />
      </FieldGroup>
      <DialogFooter>
        <Button
          type="button"
          variant="outline"
          onClick={onCancel}
          disabled={join.isPending}
        >
          {t("common.action.cancel")}
        </Button>
        <PendingButton type="submit" pending={join.isPending}>
          {join.isPending
            ? t("network.shared.submitting")
            : t("network.client.join.submit")}
        </PendingButton>
      </DialogFooter>
    </form>
  );
}
