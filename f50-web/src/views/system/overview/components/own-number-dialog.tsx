import { useState } from "react";
import { JobProgress } from "@/components/f50/job-progress";
import { Button } from "@/components/ui/button";
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
  FieldLabel,
} from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { t } from "@/i18n";
import { errorMessage } from "@/lib/format";
import type { JobStatus } from "@/lib/jobs";
import { useMutation } from "@tanstack/react-query";
import { toast } from "sonner";
import { setOwnNumber } from "../api";

/** What f50d's `own_number` accepts: an optional "+" and 3 to 15 digits (spaces and dashes are dropped here). */
const NUMBER = /^\+?\d{3,15}$/;
const clean = (v: string) => v.replace(/[\s()-]/g, "");

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
  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="sm:max-w-md">
        <DialogHeader>
          <DialogTitle>
            {t("system.overview.info.ownNumberSet.title")}
          </DialogTitle>
          <DialogDescription>
            {t("system.overview.info.ownNumberSet.description")}
          </DialogDescription>
        </DialogHeader>
        {open && (
          <OwnNumberForm
            current={current}
            onCancel={() => onOpenChange(false)}
            onSaved={() => {
              onOpenChange(false);
              onSaved();
            }}
          />
        )}
      </DialogContent>
    </Dialog>
  );
}

function OwnNumberForm({
  current,
  onCancel,
  onSaved,
}: {
  current: string | null;
  onCancel: () => void;
  onSaved: () => void;
}) {
  const [value, setValue] = useState(current ?? "");
  const [invalid, setInvalid] = useState(false);
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
    <form
      className="flex flex-col gap-4"
      onSubmit={(e) => {
        e.preventDefault();
        const n = clean(value);
        if (!NUMBER.test(n)) {
          setInvalid(true);
          return;
        }
        save.mutate(n);
      }}
    >
      <Field data-invalid={invalid}>
        <FieldLabel htmlFor="own-number">
          {t("system.overview.info.ownNumber")}
        </FieldLabel>
        <Input
          id="own-number"
          inputMode="tel"
          autoComplete="tel"
          placeholder="13800138000"
          className="font-mono text-base sm:text-sm"
          aria-invalid={invalid}
          disabled={running}
          value={value}
          onChange={(e) => {
            setValue(e.target.value);
            setInvalid(false);
          }}
        />
        <FieldDescription>
          {t("system.overview.info.ownNumberSet.hint")}
        </FieldDescription>
        {invalid && (
          <FieldError
            errors={[
              { message: t("system.overview.info.ownNumberSet.invalid") },
            ]}
          />
        )}
      </Field>
      {job && (
        <JobProgress
          id={job}
          title={t("system.overview.info.ownNumberSet.job")}
          onDone={onDone}
        />
      )}
      <DialogFooter>
        <Button type="button" variant="outline" onClick={onCancel}>
          {t("common.action.cancel")}
        </Button>
        <Button type="submit" disabled={running} aria-busy={running}>
          {t("common.action.save")}
        </Button>
      </DialogFooter>
    </form>
  );
}
