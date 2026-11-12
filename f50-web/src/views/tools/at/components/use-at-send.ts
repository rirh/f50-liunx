import { useMemo, useState } from "react";
import { t } from "@/i18n";
import { errorMessage } from "@/lib/format";
import { isJobStarted, type JobStatus } from "@/lib/jobs";
import { ApiError } from "@/lib/ubus";
import { zodResolver } from "@hookform/resolvers/zod";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { useSetAtom } from "jotai";
import { useForm } from "react-hook-form";
import { toast } from "sonner";
import { z } from "zod";
import { atKeys, atSend } from "../api";
import { CLASS_LABEL, classOf, translate } from "../messages";
import { appendOutputAtom } from "../store/terminal";
import type { AtClass, AtReply } from "../types";

const AT_MAX = 512;

const schema = () =>
  z.object({
    command: z
      .string()
      .trim()
      .min(1, t("tools.at.form.commandRequired"))
      .max(AT_MAX, t("tools.at.form.commandMax", { max: AT_MAX }))
      .regex(/^[\x20-\x7e]*$/, t("tools.at.form.commandAscii"))
      .regex(/^at/i, t("tools.at.form.commandPrefix"))
      .refine((v) => !v.includes(";"), t("tools.at.error.chaining")),
    timeout: z
      .string()
      .trim()
      .refine(
        (v) => v === "" || (/^\d+$/.test(v) && +v >= 1 && +v <= 60),
        t("tools.at.form.timeoutRange"),
      ),
  });
export type AtValues = z.infer<ReturnType<typeof schema>>;

export interface PendingConfirm {
  values: AtValues;
  cls: AtClass | null;
  message: string;
}

/**
 * Sends one command: the reply goes to the output; `missing` on `confirm` asks first and resends with
 * `confirm: true`; a scan or a long timeout comes back as a job whose result's reply lines go to the output.
 */
export function useAtSend() {
  const qc = useQueryClient();
  const append = useSetAtom(appendOutputAtom);
  const form = useForm<AtValues>({
    resolver: zodResolver(useMemo(() => schema(), [])),
    defaultValues: { command: "", timeout: "" },
  });
  const [confirm, setConfirm] = useState<PendingConfirm | null>(null);
  const [job, setJob] = useState<{ id: string; command: string } | null>(null);
  const [jobRunning, setJobRunning] = useState(false);

  const send = useMutation({
    mutationFn: ({
      values,
      confirmed,
    }: {
      values: AtValues;
      confirmed: boolean;
    }) =>
      atSend({
        command: values.command.trim(),
        ...(confirmed ? { confirm: true } : {}),
        ...(values.timeout ? { timeout: Number(values.timeout) } : {}),
      }),
    onMutate: ({ values, confirmed }) => {
      if (!confirmed)
        append({ kind: "command", lines: [values.command.trim()] });
      else append({ kind: "info", lines: [t("tools.at.log.resend")] });
    },
    onSuccess: (r, { values }) => {
      form.reset({ command: "", timeout: values.timeout });
      if (isJobStarted(r)) {
        setJob({ id: r.job, command: values.command.trim() });
        setJobRunning(true);
        append({
          kind: "info",
          lines: [t("tools.at.log.jobStarted", { job: r.job })],
        });
      } else {
        const reply = r;
        append({
          kind: "reply",
          lines: reply.reply_lines,
          status: reply.status,
        });
      }
    },
    onError: (e, { values }) => {
      if (
        e instanceof ApiError &&
        e.code === "missing" &&
        e.field === "confirm"
      ) {
        const cls = classOf(e.message);
        append({
          kind: "info",
          lines: [
            t("tools.at.log.needsConfirm", {
              reason: cls ? t(CLASS_LABEL[cls].title) : e.message,
            }),
          ],
        });
        setConfirm({ values, cls, message: e.message });
        return;
      }
      const msg =
        e instanceof ApiError ? translate(e.message) : errorMessage(e);
      append({
        kind: "error",
        lines: [
          e instanceof ApiError && e.code === "forbidden"
            ? t("tools.at.log.refused", { message: msg })
            : msg,
        ],
      });
      if (
        e instanceof ApiError &&
        (e.field === "command" || e.field === "timeout")
      ) {
        form.setError(e.field, { message: msg });
      } else toast.error(msg);
    },
    onSettled: () => qc.invalidateQueries({ queryKey: atKeys.history }),
  });

  const onJobDone = (j: JobStatus) => {
    setJobRunning(false);
    if (j.state === "done") {
      const r = j.result as AtReply | undefined;
      append({ kind: "reply", lines: r?.reply_lines ?? [], status: r?.status });
    } else {
      append({
        kind: "error",
        lines: [
          j.error
            ? translate(j.error.message)
            : j.state === "cancelled"
              ? t("tools.at.log.jobCancelled")
              : t("tools.at.log.jobUnfinished"),
        ],
      });
    }
  };

  return {
    form,
    submit: form.handleSubmit((values) =>
      send.mutate({ values, confirmed: false }),
    ),
    sending: send.isPending,
    busy: send.isPending || jobRunning,
    confirm,
    acceptConfirm: () => {
      if (confirm) send.mutate({ values: confirm.values, confirmed: true });
      setConfirm(null);
    },
    rejectConfirm: () => {
      append({ kind: "info", lines: [t("tools.at.log.sendCancelled")] });
      setConfirm(null);
    },
    job,
    jobRunning,
    onJobDone,
    dismissJob: () => setJob(null),
  };
}
