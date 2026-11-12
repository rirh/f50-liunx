import { ConfirmButton } from "@/components/f50/confirm-button";
import { KvList } from "@/components/f50/kv-list";
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert";
import { Badge } from "@/components/ui/badge";
import { Checkbox } from "@/components/ui/checkbox";
import {
  Field,
  FieldError,
  FieldGroup,
  FieldLabel,
} from "@/components/ui/field";
import { errorMessage, formatBytes } from "@/lib/format";
import { ApiError } from "@/lib/ubus";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { Controller, useForm } from "react-hook-form";
import { toast } from "sonner";
import { UPDATE_KEY, applyFile, uploadCancel } from "../api";
import { useStartJob } from "../hooks";
import { SIG_LABEL } from "../labels";
import type { Inspected } from "../types";

interface FormValues {
  unsigned: boolean;
  downgrade: boolean;
}

const yesNo = (ok: boolean, yes: string, no: string) => (
  <Badge variant={ok ? "secondary" : "destructive"}>{ok ? yes : no}</Badge>
);

/** What `upload_done` found the file to be, and installing it (each thing wrong with it confirmed first). */
export function InspectResult({
  inspected: r,
  busy,
}: {
  inspected: Inspected;
  busy: boolean;
}) {
  const qc = useQueryClient();
  const started = useStartJob();
  const form = useForm<FormValues>({
    defaultValues: { unsigned: false, downgrade: false },
  });
  const errors = form.formState.errors;
  const sigRefused = r.signature === "bad" || r.signature === "badkey";
  const needsUnsigned = r.signature !== "valid" && !sigRefused;
  const usable = !r.refused && r.fits && r.whole && !sigRefused;

  const install = useMutation({
    mutationFn: (v: FormValues) => applyFile(r.sha256, v),
    onSuccess: started(`安装离线更新包 ${r.tag ?? ""}`.trim()),
    onError: (e) => {
      if (
        e instanceof ApiError &&
        (e.field === "unsigned" || e.field === "downgrade")
      ) {
        form.setError(e.field, { message: errorMessage(e) });
      } else toast.error(errorMessage(e));
    },
  });
  const cancel = useMutation({
    mutationFn: uploadCancel,
    onSuccess: () => toast.success("已删除上传的文件"),
    onError: (e) => toast.error(errorMessage(e)),
    onSettled: () => qc.invalidateQueries({ queryKey: UPDATE_KEY }),
  });

  const submit = form.handleSubmit((v) => {
    if (needsUnsigned && !v.unsigned)
      return form.setError("unsigned", { message: "未签名的更新包需要确认" });
    if (r.downgrade && !v.downgrade)
      return form.setError("downgrade", { message: "降级需要确认" });
    install.mutate(v);
  });

  return (
    <div className="flex flex-col gap-3 rounded-lg border p-3">
      <KvList
        items={[
          {
            label: "版本",
            value: <span className="font-mono">{r.tag ?? "未知"}</span>,
          },
          { label: "大小", value: formatBytes(r.size) },
          {
            label: "SHA-256",
            value: (
              <span className="font-mono text-xs break-all">{r.sha256}</span>
            ),
          },
          {
            label: "签名",
            value: (
              <Badge
                variant={r.signature === "valid" ? "secondary" : "destructive"}
              >
                {SIG_LABEL[r.signature] ?? r.signature}
              </Badge>
            ),
          },
          {
            label: "文件完整",
            value: yesNo(r.whole, "全部校验通过", "有文件校验失败"),
          },
          {
            label: "适用本机",
            value: r.fits
              ? `将更新：${r.systems.join("、")}`
              : yesNo(false, "", "不适用于此设备"),
          },
          { label: "包含内核", value: r.kernel ? "是" : "否" },
          {
            label: "降级",
            value: r.downgrade ? (
              <Badge variant="destructive">比已安装版本旧</Badge>
            ) : (
              "否"
            ),
          },
        ]}
      />
      {r.refused && (
        <Alert variant="destructive">
          <AlertTitle>此文件无法使用</AlertTitle>
          <AlertDescription>{r.refused}</AlertDescription>
        </Alert>
      )}
      {usable && (needsUnsigned || r.downgrade) && (
        <FieldGroup>
          {needsUnsigned && (
            <Controller
              control={form.control}
              name="unsigned"
              render={({ field }) => (
                <Field
                  orientation="horizontal"
                  data-invalid={!!errors.unsigned || undefined}
                >
                  <Checkbox
                    id="apply-unsigned"
                    checked={field.value}
                    onCheckedChange={(v) => field.onChange(v === true)}
                    aria-invalid={!!errors.unsigned || undefined}
                  />
                  <FieldLabel htmlFor="apply-unsigned">
                    我确认安装未经签名校验的更新包
                  </FieldLabel>
                  {errors.unsigned && (
                    <FieldError>{errors.unsigned.message}</FieldError>
                  )}
                </Field>
              )}
            />
          )}
          {r.downgrade && (
            <Controller
              control={form.control}
              name="downgrade"
              render={({ field }) => (
                <Field
                  orientation="horizontal"
                  data-invalid={!!errors.downgrade || undefined}
                >
                  <Checkbox
                    id="apply-downgrade"
                    checked={field.value}
                    onCheckedChange={(v) => field.onChange(v === true)}
                    aria-invalid={!!errors.downgrade || undefined}
                  />
                  <FieldLabel htmlFor="apply-downgrade">
                    我确认降级到较旧的版本
                  </FieldLabel>
                  {errors.downgrade && (
                    <FieldError>{errors.downgrade.message}</FieldError>
                  )}
                </Field>
              )}
            />
          )}
        </FieldGroup>
      )}
      <div className="flex flex-wrap gap-2">
        <ConfirmButton
          size="sm"
          destructive={false}
          pending={install.isPending}
          disabled={busy || !usable || cancel.isPending}
          title={`安装 ${r.tag ?? "此更新包"}？`}
          confirmText="开始安装"
          description={
            <span className="flex flex-col gap-2">
              <span>
                将写入：{r.systems.join("、") || "—"}
                {r.kernel ? "，以及内核" : ""}。完成后需重启生效。
              </span>
              <span className="font-medium text-destructive">
                安装开始后无法停止。期间请保持电源连接，不要重启或拔出设备，否则可能导致无法启动。
              </span>
            </span>
          }
          onConfirm={() => void submit()}
        >
          {install.isPending ? "启动中" : "安装此更新包"}
        </ConfirmButton>
        <ConfirmButton
          size="sm"
          variant="outline"
          pending={cancel.isPending}
          disabled={busy || install.isPending}
          title="删除上传的文件？"
          description="删除后需要重新上传才能安装。"
          confirmText="删除"
          onConfirm={() => cancel.mutate()}
        >
          删除上传
        </ConfirmButton>
      </div>
    </div>
  );
}
