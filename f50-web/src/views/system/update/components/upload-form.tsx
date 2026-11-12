import { useState } from "react";
import { Button } from "@/components/ui/button";
import {
  Field,
  FieldDescription,
  FieldError,
  FieldGroup,
  FieldLabel,
} from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { Progress } from "@/components/ui/progress";
import { SweepShine } from "@/components/ui/sweep-shine";
import { useSession } from "@/hooks/use-session";
import { type Key, t } from "@/i18n";
import { errorMessage, formatBytes } from "@/lib/format";
import { ApiError } from "@/lib/ubus";
import { useQueryClient } from "@tanstack/react-query";
import { UploadIcon, XIcon } from "lucide-react";
import { useForm, useWatch } from "react-hook-form";
import { toast } from "sonner";
import { UPDATE_KEY, uploadBegin, uploadDone, uploadFile } from "../api";
import { useStartJob } from "../hooks";
import { nativeDigest, sha256File } from "../lib/sha256";

/** The largest bundle `upload_begin` takes (2047 MiB). */
const MAX_SIZE = 2047 * 1024 * 1024;

type Phase =
  | { step: "idle" }
  | { step: "hashing"; done: number; total: number }
  | { step: "begin" }
  | { step: "uploading" }
  | { step: "checking" };

const STEP_TEXT = {
  begin: "maintenance.update.upload.step.begin",
  uploading: "maintenance.update.upload.step.uploading",
  checking: "maintenance.update.upload.step.checking",
} satisfies Record<string, Key>;

interface FormValues {
  file: File | null;
}

/** Choose a release bundle (.tar), hash it here, announce it, upload it, and have the device look at it. */
export function UploadForm({ busy }: { busy: boolean }) {
  const session = useSession();
  const qc = useQueryClient();
  const started = useStartJob();
  const [phase, setPhase] = useState<Phase>({ step: "idle" });
  const [abort, setAbort] = useState<AbortController | null>(null);
  // a new key clears the file input after an upload
  const [inputKey, setInputKey] = useState(0);
  const form = useForm<FormValues>({ defaultValues: { file: null } });
  const file = useWatch({ control: form.control, name: "file" });
  const fileError = form.formState.errors.file;
  const working = phase.step !== "idle";

  async function onSubmit({ file }: FormValues) {
    if (!file) {
      form.setError("file", {
        message: t("maintenance.update.upload.required"),
      });
      return;
    }
    if (!session) {
      toast.error(t("app.error.sessionExpired"));
      return;
    }
    const ctl = new AbortController();
    setAbort(ctl);
    try {
      setPhase({ step: "hashing", done: 0, total: file.size });
      const sha = await sha256File(
        file,
        (done) => setPhase({ step: "hashing", done, total: file.size }),
        ctl.signal,
      );
      setPhase({ step: "begin" });
      const { path } = await uploadBegin(file.size, sha);
      setPhase({ step: "uploading" });
      const reply = await uploadFile(session.id, path, file);
      if (reply.size != null && reply.size !== file.size) {
        throw new Error(
          t("maintenance.update.upload.incomplete", {
            got: formatBytes(reply.size),
            size: formatBytes(file.size),
          }),
        );
      }
      setPhase({ step: "checking" });
      started(t("maintenance.update.jobTitle.uploadDone"))(await uploadDone());
      form.reset({ file: null });
      setInputKey((k) => k + 1);
    } catch (e) {
      if (e instanceof DOMException && e.name === "AbortError")
        toast.info(t("maintenance.update.upload.cancelled"));
      else if (e instanceof ApiError && e.field)
        form.setError("file", { message: errorMessage(e) });
      else toast.error(errorMessage(e));
    } finally {
      setAbort(null);
      setPhase({ step: "idle" });
      qc.invalidateQueries({ queryKey: UPDATE_KEY });
    }
  }

  const pct =
    phase.step === "hashing" && phase.total > 0
      ? (phase.done / phase.total) * 100
      : null;

  return (
    <form onSubmit={form.handleSubmit(onSubmit)} noValidate>
      <FieldGroup>
        <Field
          data-invalid={!!fileError || undefined}
          data-disabled={working || busy || undefined}
        >
          <FieldLabel htmlFor="update-file">
            {t("maintenance.update.upload.label")}
          </FieldLabel>
          <Input
            id="update-file"
            key={inputKey}
            type="file"
            accept=".tar,application/x-tar"
            disabled={working || busy}
            aria-invalid={!!fileError || undefined}
            onChange={(e) => {
              const f = e.target.files?.[0] ?? null;
              form.clearErrors("file");
              form.setValue("file", f);
              if (f && f.size > MAX_SIZE)
                form.setError("file", {
                  message: t("maintenance.update.upload.tooLarge"),
                });
            }}
          />
          {fileError ? (
            <FieldError>{fileError.message}</FieldError>
          ) : (
            <FieldDescription>
              {t("maintenance.update.upload.hint")}
              {file &&
                ` ${t("maintenance.update.upload.chosen", {
                  name: file.name,
                  size: formatBytes(file.size),
                })}`}
              {nativeDigest()
                ? ` ${t("maintenance.update.upload.nativeHash")}`
                : ` ${t("maintenance.update.upload.slowHash")}`}
            </FieldDescription>
          )}
        </Field>
        {phase.step === "hashing" && (
          <Field>
            <FieldDescription>
              <SweepShine>{t("maintenance.update.upload.hashing")}</SweepShine>
              {t("maintenance.update.upload.hashProgress", {
                done: formatBytes(phase.done),
                total: formatBytes(phase.total),
              })}
            </FieldDescription>
            <Progress
              value={pct ?? 0}
              aria-label={t("maintenance.update.upload.hashAria")}
            />
          </Field>
        )}
        {(phase.step === "begin" ||
          phase.step === "uploading" ||
          phase.step === "checking") && (
          <Field>
            <FieldDescription>
              <SweepShine>{t(STEP_TEXT[phase.step])}</SweepShine>
              {phase.step === "uploading" &&
                file &&
                t("maintenance.update.upload.keepOpen", {
                  size: formatBytes(file.size),
                })}
            </FieldDescription>
            <Progress
              value={
                phase.step === "uploading"
                  ? 66
                  : phase.step === "checking"
                    ? 90
                    : 33
              }
              aria-label={t("maintenance.update.upload.progressAria")}
            />
          </Field>
        )}
        <Field orientation="horizontal">
          <Button
            type="submit"
            size="sm"
            disabled={busy || working || !file || !!fileError}
            aria-busy={working}
          >
            <UploadIcon data-icon="inline-start" />
            {working ? (
              <SweepShine>{t("maintenance.update.upload.working")}</SweepShine>
            ) : (
              t("maintenance.update.upload.submit")
            )}
          </Button>
          {phase.step === "hashing" && (
            <Button
              type="button"
              size="sm"
              variant="outline"
              onClick={() => abort?.abort()}
            >
              <XIcon data-icon="inline-start" />
              {t("common.action.cancel")}
            </Button>
          )}
        </Field>
      </FieldGroup>
    </form>
  );
}
