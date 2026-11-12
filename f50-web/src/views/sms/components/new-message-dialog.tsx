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
  FieldGroup,
  FieldLabel,
} from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { SweepShine } from "@/components/ui/sweep-shine";
import { Textarea } from "@/components/ui/textarea";
import { useAtom } from "jotai";
import { SendIcon } from "lucide-react";
import { SmsCounter } from "./sms-counter";
import { useSendForm } from "./use-send-form";
import { composeOpenAtom } from "../store/sms";

/** A message to any number; once sent, its conversation opens. */
export function NewMessageDialog({
  onSent,
}: {
  onSent: (peer: string) => void;
}) {
  const [open, setOpen] = useAtom(composeOpenAtom);
  return (
    <Dialog open={open} onOpenChange={setOpen}>
      <DialogContent className="sm:max-w-lg">
        <DialogHeader>
          <DialogTitle>新短信</DialogTitle>
          <DialogDescription>
            最多 10 条拼接发送：GSM-7 每条 160 字（拼接时
            153），含中文等字符时每条 70 字（67）。
          </DialogDescription>
        </DialogHeader>
        {open && (
          <NewMessageForm
            onSent={(peer) => {
              setOpen(false);
              onSent(peer);
            }}
            onCancel={() => setOpen(false)}
          />
        )}
      </DialogContent>
    </Dialog>
  );
}

function NewMessageForm({
  onSent,
  onCancel,
}: {
  onSent: (peer: string) => void;
  onCancel: () => void;
}) {
  const { form, submit, pending, job, onJobDone } = useSendForm("", onSent);
  const { errors } = form.formState;
  const text = form.watch("text");
  return (
    <form onSubmit={submit} className="flex flex-col gap-4">
      <FieldGroup>
        <Field data-invalid={!!errors.number}>
          <FieldLabel htmlFor="sms-number">号码</FieldLabel>
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
          <FieldLabel htmlFor="sms-text">内容</FieldLabel>
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
      {job && <JobProgress id={job.id} title="发送短信" onDone={onJobDone} />}
      <DialogFooter>
        <Button type="button" variant="outline" onClick={onCancel}>
          取消
        </Button>
        <Button type="submit" disabled={pending} aria-busy={pending}>
          <SendIcon data-icon="inline-start" />
          {pending ? <SweepShine>发送中</SweepShine> : "发送"}
        </Button>
      </DialogFooter>
    </form>
  );
}
