import { PendingButton } from "@/components/f50/pending-button";
import {
  Field,
  FieldDescription,
  FieldError,
  FieldLabel,
} from "@/components/ui/field";
import { Textarea } from "@/components/ui/textarea";
import { reportFormError } from "@/lib/form-error";
import { zodResolver } from "@hookform/resolvers/zod";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { Controller, useForm } from "react-hook-form";
import { toast } from "sonner";
import { z } from "zod";
import { isTextLine } from "../../shared/text-rules";
import { addAdbKey, usbKeys } from "../api";

/** `adbd/auth.rs key_ok`: 700 base64 characters, `QAAAA` first, `=` padded; then an optional `user@host`. */
function keyOk(line: string): boolean {
  const t = line.trim();
  const i = t.indexOf(" ");
  const k = i < 0 ? t : t.slice(0, i);
  return (
    k.length === 700 && k.startsWith("QAAAA") && /^[A-Za-z0-9+/]+=$/.test(k)
  );
}

const schema = z.object({
  key: z
    .string()
    .min(1, "请粘贴公钥")
    .max(800, "最多 800 个字符")
    .refine((v) => isTextLine(v) && !v.startsWith("-"), "只能是一行文本")
    .refine(
      keyOk,
      "不是有效的 adb 公钥（~/.android/adbkey.pub 的内容，以 QAAAA 开头）",
    ),
});
type KeyForm = z.infer<typeof schema>;

export function AdbKeyForm() {
  const qc = useQueryClient();
  const form = useForm<KeyForm>({
    resolver: zodResolver(schema),
    defaultValues: { key: "" },
    mode: "onTouched",
  });
  const add = useMutation({
    mutationFn: (v: KeyForm) => addAdbKey(v.key.trim()),
    onSuccess: (r) => {
      if (r.added) toast.success(`已授权，共 ${r.keys} 个密钥`);
      else toast.info("该密钥已授权");
      form.reset({ key: "" });
      qc.invalidateQueries({ queryKey: usbKeys.info });
    },
    onError: (e) => reportFormError(e, form.setError, ["key"]),
  });
  return (
    <form
      onSubmit={form.handleSubmit((v) => add.mutate(v))}
      noValidate
      className="flex flex-col gap-3"
    >
      <Controller
        control={form.control}
        name="key"
        render={({ field, fieldState }) => (
          <Field data-invalid={fieldState.invalid}>
            <FieldLabel htmlFor="adb-key">授权电脑公钥</FieldLabel>
            <Textarea
              id="adb-key"
              rows={4}
              spellCheck={false}
              className="font-mono text-xs break-all"
              placeholder="QAAAA…= user@host"
              aria-invalid={fieldState.invalid}
              {...field}
            />
            {fieldState.error ? (
              <FieldError errors={[fieldState.error]} />
            ) : (
              <FieldDescription>
                粘贴电脑上 ~/.android/adbkey.pub 的内容
              </FieldDescription>
            )}
          </Field>
        )}
      />
      <PendingButton
        type="submit"
        size="sm"
        className="self-end"
        pending={add.isPending}
      >
        {add.isPending ? "添加中" : "添加密钥"}
      </PendingButton>
    </form>
  );
}
