import { useState } from "react";
import { ResponsiveDialog } from "@/components/f50/form-dialog";
import { JobProgress } from "@/components/f50/job-progress";
import { Button } from "@/components/ui/button";
import {
  Field,
  FieldDescription,
  FieldError,
  FieldGroup,
  FieldLabel,
} from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { SweepShine } from "@/components/ui/sweep-shine";
import { Textarea } from "@/components/ui/textarea";
import { t } from "@/i18n";
import { useAtom } from "jotai";
import { SendIcon } from "lucide-react";
import { SmsCounter } from "./sms-counter";
import { useSendForm } from "./use-send-form";
import { composeOpenAtom } from "../store/sms";

const FORM_ID = "sms-new-message";

/**
 * Counts openings. The panel is keyed by it, so every opening starts with an empty form and no send job left over,
 * as before; closing keeps the same panel for the drawer's or dialog's exit animation.
 */
function useOpenCount(open: boolean) {
  const [count, setCount] = useState(0);
  const [wasOpen, setWasOpen] = useState(open);
  if (open !== wasOpen) {
    setWasOpen(open);
    if (open) setCount(count + 1);
  }
  return count;
}

/** A message to any number; once sent, its conversation opens. A dialog on a wide screen, a bottom drawer on a phone. */
export function NewMessageDialog({
  onSent,
}: {
  onSent: (peer: string) => void;
}) {
  const [open, setOpen] = useAtom(composeOpenAtom);
  const session = useOpenCount(open);
  return (
    <NewMessagePanel
      key={session}
      open={open}
      onOpenChange={setOpen}
      onSent={(peer) => {
        setOpen(false);
        onSent(peer);
      }}
    />
  );
}

function NewMessagePanel({
  open,
  onOpenChange,
  onSent,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  onSent: (peer: string) => void;
}) {
  const { form, submit, pending, job, onJobDone } = useSendForm("", onSent);
  const { errors } = form.formState;
  const text = form.watch("text");
  return (
    <ResponsiveDialog
      open={open}
      onOpenChange={onOpenChange}
      title={t("sms.newMessage.title")}
      description={t("sms.newMessage.description")}
      footer={
        <>
          <Button
            type="button"
            variant="outline"
            onClick={() => onOpenChange(false)}
          >
            {t("common.action.cancel")}
          </Button>
          <Button
            type="submit"
            form={FORM_ID}
            disabled={pending}
            aria-busy={pending}
          >
            <SendIcon data-icon="inline-start" />
            {pending ? (
              <SweepShine>{t("sms.composer.sending")}</SweepShine>
            ) : (
              t("sms.composer.send")
            )}
          </Button>
        </>
      }
    >
      <form id={FORM_ID} onSubmit={submit} className="flex flex-col gap-4">
        <FieldGroup>
          <Field data-invalid={!!errors.number}>
            <FieldLabel htmlFor="sms-number">
              {t("sms.newMessage.number")}
            </FieldLabel>
            <Input
              id="sms-number"
              inputMode="tel"
              autoComplete="tel"
              placeholder="+8613800138000"
              className="text-base sm:text-sm"
              aria-invalid={!!errors.number}
              disabled={pending}
              {...form.register("number")}
            />
            <FieldError errors={[errors.number]} />
          </Field>
          <Field data-invalid={!!errors.text}>
            <FieldLabel htmlFor="sms-text">
              {t("sms.newMessage.text")}
            </FieldLabel>
            <Textarea
              id="sms-text"
              rows={5}
              className="max-h-64 text-base sm:text-sm"
              aria-invalid={!!errors.text}
              disabled={pending}
              {...form.register("text")}
            />
            <FieldDescription>
              <SmsCounter text={text} />
            </FieldDescription>
            <FieldError errors={[errors.text]} />
          </Field>
        </FieldGroup>
        {job && (
          <JobProgress
            id={job.id}
            title={t("sms.composer.jobTitle")}
            onDone={onJobDone}
          />
        )}
      </form>
    </ResponsiveDialog>
  );
}
