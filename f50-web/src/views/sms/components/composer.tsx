import { JobProgress } from "@/components/f50/job-progress";
import { Button } from "@/components/ui/button";
import { Field, FieldError } from "@/components/ui/field";
import {
  InputGroup,
  InputGroupAddon,
  InputGroupButton,
  InputGroupTextarea,
} from "@/components/ui/input-group";
import { Spinner } from "@/components/ui/spinner";
import { t } from "@/i18n";
import { ArrowUpIcon, XIcon } from "lucide-react";
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
      <p className="mx-auto mb-4 w-full max-w-3xl px-4 text-center text-xs text-muted-foreground sm:px-6">
        {t("sms.composer.notNumber")}
      </p>
    );
  }

  return (
    <form
      onSubmit={submit}
      className="mx-auto flex w-full max-w-3xl shrink-0 flex-col gap-1.5 px-4 pt-1 pb-4 sm:px-6"
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
        <InputGroup className="min-h-9 rounded-[20px] border-transparent bg-foreground/[0.05] ps-1.5 pe-1 transition-colors has-[[data-slot=input-group-control]:focus-visible]:border-transparent has-[[data-slot=input-group-control]:focus-visible]:bg-foreground/[0.07] has-[[data-slot=input-group-control]:focus-visible]:ring-0 dark:bg-foreground/10">
          <InputGroupTextarea
            aria-label={t("sms.composer.textLabel")}
            placeholder={t("sms.composer.placeholder")}
            rows={1}
            className="field-sizing-content max-h-40 min-h-9 py-2 text-[15px] leading-[1.35]"
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
          {(text || pending) && (
            <InputGroupAddon
              align="inline-end"
              className="gap-2 self-end pe-0 pb-1"
            >
              <SmsCounter
                text={text}
                className="hidden text-[11px] sm:inline"
              />
              <InputGroupButton
                type="submit"
                variant="default"
                size="icon-sm"
                className="size-7 rounded-full"
                title={`${t("sms.composer.send")} (Ctrl/⌘ + Enter)`}
                aria-label={t("sms.composer.send")}
                disabled={pending}
                aria-busy={pending}
              >
                {pending ? <Spinner /> : <ArrowUpIcon />}
              </InputGroupButton>
            </InputGroupAddon>
          )}
        </InputGroup>
        <FieldError errors={[error]} />
      </Field>
    </form>
  );
}
