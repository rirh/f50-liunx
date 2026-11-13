import { useMemo, useState } from "react";
import { FormDialog } from "@/components/f50/form-dialog";
import { JobProgress } from "@/components/f50/job-progress";
import {
  Field,
  FieldDescription,
  FieldError,
  FieldLabel,
} from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { t } from "@/i18n";
import { errorMessage } from "@/lib/format";
import type { JobStatus } from "@/lib/jobs";
import { zodResolver } from "@hookform/resolvers/zod";
import { useMutation } from "@tanstack/react-query";
import { Controller, useForm } from "react-hook-form";
import { toast } from "sonner";
import { z } from "zod";
import { setOwnNumber } from "../api";

/** What f50d's `own_number` accepts: an optional "+" and 3 to 15 digits (spaces and dashes are dropped here). */
const NUMBER = /^\+?\d{3,15}$/;
const clean = (v: string) => v.replace(/[\s()-]/g, "");

const ownNumberSchema = () =>
  z.object({
    number: z
      .string()
      .refine(
        (v) => NUMBER.test(clean(v)),
        t("system.overview.info.ownNumberSet.invalid"),
      ),
  });
type OwnNumberValues = z.infer<ReturnType<typeof ownNumberSchema>>;

/**
 * Counts openings. The panel is keyed by it, so every opening starts fresh (the current number, no job left over from
 * an earlier attempt) while closing keeps the same panel for the drawer's or dialog's exit animation.
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

/** Writes the SIM's own number to the card, for SIMs that do not carry it (AT+CNUM finds nothing). */
export function OwnNumberDialog({
  open,
  onOpenChange,
  current,
  onSaved,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  current: string | null;
  onSaved: () => void;
}) {
  const session = useOpenCount(open);
  return (
    <OwnNumberPanel
      key={session}
      open={open}
      onOpenChange={onOpenChange}
      current={current}
      onSaved={() => {
        onOpenChange(false);
        onSaved();
      }}
    />
  );
}

function OwnNumberPanel({
  open,
  onOpenChange,
  current,
  onSaved,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  current: string | null;
  onSaved: () => void;
}) {
  const form = useForm<OwnNumberValues>({
    resolver: zodResolver(useMemo(() => ownNumberSchema(), [])),
    defaultValues: { number: current ?? "" },
  });
  const [job, setJob] = useState<string | null>(null);
  const save = useMutation({
    mutationFn: setOwnNumber,
    onSuccess: (r) => setJob(r.job),
    onError: (e) => toast.error(errorMessage(e)),
  });
  const running = save.isPending || !!job;
  const onDone = (j: JobStatus) => {
    setJob(null);
    if (j.state === "done") {
      toast.success(t("system.overview.info.ownNumberSet.saved"));
      onSaved();
    }
  };
  return (
    <FormDialog
      open={open}
      onOpenChange={onOpenChange}
      title={t("system.overview.info.ownNumberSet.title")}
      description={t("system.overview.info.ownNumberSet.description")}
      formId="own-number-form"
      size="sm"
      // only the request itself blocks closing: a job that never reports back must not trap the dialog
      pending={save.isPending}
      dirty={form.formState.isDirty && !job}
    >
      <form
        id="own-number-form"
        className="flex flex-col gap-4"
        onSubmit={form.handleSubmit((v) => save.mutate(clean(v.number)))}
      >
        <Controller
          name="number"
          control={form.control}
          render={({ field, fieldState }) => (
            <Field data-invalid={fieldState.invalid}>
              <FieldLabel htmlFor="own-number">
                {t("system.overview.info.ownNumber")}
              </FieldLabel>
              <Input
                {...field}
                id="own-number"
                inputMode="tel"
                autoComplete="tel"
                placeholder="13800138000"
                className="font-mono text-base sm:text-sm"
                aria-invalid={fieldState.invalid}
                disabled={running}
              />
              <FieldDescription>
                {t("system.overview.info.ownNumberSet.hint")}
              </FieldDescription>
              {fieldState.invalid && <FieldError errors={[fieldState.error]} />}
            </Field>
          )}
        />
        {job && (
          <JobProgress
            id={job}
            title={t("system.overview.info.ownNumberSet.job")}
            onDone={onDone}
          />
        )}
      </form>
    </FormDialog>
  );
}
