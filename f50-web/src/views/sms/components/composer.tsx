import { JobProgress } from "@/components/f50/job-progress";
import { Button } from "@/components/ui/button";
import { Field, FieldError } from "@/components/ui/field";
import {
  InputGroup,
  InputGroupAddon,
  InputGroupButton,
  InputGroupTextarea,
} from "@/components/ui/input-group";
import { SweepShine } from "@/components/ui/sweep-shine";
import { t } from "@/i18n";
import { SendIcon, XIcon } from "lucide-react";
import { SmsCounter } from "./sms-counter";
import { useSendForm } from "./use-send-form";
import { NUMBER_RE } from "../lib/encoding";

/** The reply box under a thread; Ctrl/⌘+Enter sends. */
export function Composer({ peer }: { peer: string }) {
  const { form, submit, pending, job, running, onJobDone, dismissJob } =
    useSendForm(peer);
  const text = form.watch("text");
  const error = form.formState.errors.text ?? form.formState.errors.number;

  if (!NUMBER_RE.test(peer)) {
    return (
      <p className="border-t p-3 text-sm text-muted-foreground">
        {t("sms.composer.notNumber")}
      </p>
    );
  }

  return (
    <form onSubmit={submit} className="flex flex-col gap-2 border-t p-3">
      {job && (
        <div className="flex items-start gap-2">
          <div className="min-w-0 flex-1">
            <JobProgress
              id={job.id}
              title={t("sms.composer.jobTitle")}
              onDone={onJobDone}
            />
          </div>
          {!running && (
            <Button
              type="button"
              variant="ghost"
              size="icon-sm"
              aria-label={t("common.action.close")}
              onClick={dismissJob}
            >
              <XIcon />
            </Button>
          )}
        </div>
      )}
      <Field data-invalid={!!error}>
        <InputGroup>
          <InputGroupTextarea
            aria-label={t("sms.composer.textLabel")}
            placeholder={t("sms.composer.placeholder")}
            rows={2}
            className="max-h-40 min-h-12 text-base sm:text-sm"
            aria-invalid={!!error}
            disabled={pending}
            onKeyDown={(e) => {
              if (e.key === "Enter" && (e.ctrlKey || e.metaKey)) {
                e.preventDefault();
                void submit();
              }
            }}
            {...form.register("text")}
          />
          <InputGroupAddon align="block-end">
            <SmsCounter text={text} />
            <InputGroupButton
              type="submit"
              variant="default"
              size="sm"
              className="ml-auto"
              disabled={pending}
              aria-busy={pending}
            >
              <SendIcon data-icon="inline-start" />
              {pending ? (
                <SweepShine>{t("sms.composer.sending")}</SweepShine>
              ) : (
                t("sms.composer.send")
              )}
            </InputGroupButton>
          </InputGroupAddon>
        </InputGroup>
        <FieldError errors={[error]} />
      </Field>
    </form>
  );
}
