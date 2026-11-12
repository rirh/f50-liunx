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
  begin: "正在申请上传空间",
  uploading: "正在上传",
  checking: "正在提交检查",
} as const;

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
      form.setError("file", { message: "请选择更新包文件" });
      return;
    }
    if (!session) {
      toast.error("登录已过期，请重新登录");
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
          `上传不完整（${formatBytes(reply.size)} / ${formatBytes(file.size)}）`,
        );
      }
      setPhase({ step: "checking" });
      started("检查上传的文件")(await uploadDone());
      form.reset({ file: null });
      setInputKey((k) => k + 1);
    } catch (e) {
      if (e instanceof DOMException && e.name === "AbortError")
        toast.info("已取消");
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
          <FieldLabel htmlFor="update-file">离线更新包</FieldLabel>
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
                form.setError("file", { message: "文件超过 2047 MB" });
            }}
          />
          {fileError ? (
            <FieldError>{fileError.message}</FieldError>
          ) : (
            <FieldDescription>
              发布页面提供的整版 tar 包（含 SHA256SUMS 与签名）。
              {file && ` 已选：${file.name}，${formatBytes(file.size)}。`}
              {nativeDigest()
                ? " 校验值计算时会把整个文件读入浏览器内存。"
                : " 当前为 http 访问，校验值由页面逐块计算，速度较慢。"}
            </FieldDescription>
          )}
        </Field>
        {phase.step === "hashing" && (
          <Field>
            <FieldDescription>
              <SweepShine>正在计算 SHA-256</SweepShine>（
              {formatBytes(phase.done)} / {formatBytes(phase.total)}）
            </FieldDescription>
            <Progress value={pct ?? 0} aria-label="SHA-256 计算进度" />
          </Field>
        )}
        {(phase.step === "begin" ||
          phase.step === "uploading" ||
          phase.step === "checking") && (
          <Field>
            <FieldDescription>
              <SweepShine>{STEP_TEXT[phase.step]}</SweepShine>
              {phase.step === "uploading" &&
                file &&
                `（${formatBytes(file.size)}，请勿关闭页面）`}
            </FieldDescription>
            <Progress
              value={
                phase.step === "uploading"
                  ? 66
                  : phase.step === "checking"
                    ? 90
                    : 33
              }
              aria-label="上传进度"
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
            {working ? <SweepShine>处理中</SweepShine> : "上传并检查"}
          </Button>
          {phase.step === "hashing" && (
            <Button
              type="button"
              size="sm"
              variant="outline"
              onClick={() => abort?.abort()}
            >
              <XIcon data-icon="inline-start" />
              取消
            </Button>
          )}
        </Field>
      </FieldGroup>
    </form>
  );
}
