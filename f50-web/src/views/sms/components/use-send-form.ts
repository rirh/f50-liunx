import { useState } from "react";
import { errorMessage } from "@/lib/format";
import type { JobStatus } from "@/lib/jobs";
import { ApiError } from "@/lib/ubus";
import { zodResolver } from "@hookform/resolvers/zod";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { useForm } from "react-hook-form";
import { toast } from "sonner";
import { z } from "zod";
import { sendSms, smsKeys } from "../api";
import { NUMBER_RE, smsTextProblem } from "../lib/encoding";
import type { SendResult } from "../types";

const schema = z.object({
  number: z
    .string()
    .trim()
    .regex(NUMBER_RE, "号码只能是数字，可带开头的 +，最多 20 位"),
  text: z.string().superRefine((v, ctx) => {
    const p = smsTextProblem(v);
    if (p) ctx.addIssue({ code: "custom", message: p });
  }),
});
export type SendValues = z.infer<typeof schema>;

/**
 * The send form both the composer and the new-message dialog use: validation as f50d checks it, the send job's
 * id while it runs, f50d's field errors on their fields.
 */
export function useSendForm(number: string, onSent?: (number: string) => void) {
  const qc = useQueryClient();
  const form = useForm<SendValues>({
    resolver: zodResolver(schema),
    defaultValues: { number, text: "" },
  });
  const [job, setJob] = useState<{ id: string; number: string } | null>(null);
  const [running, setRunning] = useState(false);

  const send = useMutation({
    mutationFn: (v: SendValues) => sendSms(v.number.trim(), v.text),
    onSuccess: (r, v) => {
      setJob({ id: r.job, number: v.number.trim() });
      setRunning(true);
    },
    onError: (e) => {
      if (
        e instanceof ApiError &&
        (e.field === "number" || e.field === "text")
      ) {
        form.setError(e.field, { message: errorMessage(e) });
      } else toast.error(errorMessage(e));
    },
  });

  const onJobDone = (j: JobStatus) => {
    setRunning(false);
    void qc.invalidateQueries({ queryKey: smsKeys.all });
    if (j.state === "done") {
      const r = j.result as SendResult | undefined;
      toast.success(r && r.parts > 1 ? `已发送（${r.parts} 条）` : "已发送");
      form.reset({ number: form.getValues("number"), text: "" });
      setJob(null);
      if (job) onSent?.(job.number);
    } else {
      toast.error(j.error?.message ?? "发送失败");
    }
  };

  return {
    form,
    submit: form.handleSubmit((v) => send.mutate(v)),
    pending: send.isPending || running,
    job,
    running,
    onJobDone,
    dismissJob: () => setJob(null),
  };
}
