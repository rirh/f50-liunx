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
      <p className="border-t px-3 py-2 text-xs text-muted-foreground">
        {t("sms.composer.notNumber")}
      </p>
    );
  }

  return (
    <form
      onSubmit={submit}
      className="flex shrink-0 flex-col gap-1.5 border-t p-2"
    >
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
            rows={1}
            className="field-sizing-content max-h-32 min-h-9 py-2 text-base sm:text-sm"
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
          <InputGroupAddon align="inline-end" className="self-end pb-1.5">
            <SmsCounter text={text} className="hidden text-[11px] sm:inline" />
            <InputGroupButton
              type="submit"
              variant="default"
              size="xs"
              title="Ctrl/⌘ + Enter"
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
