import { ConfirmButton } from "@/components/f50/confirm-button";
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert";
import { Button } from "@/components/ui/button";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import { SweepShine } from "@/components/ui/sweep-shine";
import { errorMessage } from "@/lib/format";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { toast } from "sonner";
import { InspectResult } from "./inspect-result";
import { UploadForm } from "./upload-form";
import { UPDATE_KEY, uploadCancel, uploadDone } from "../api";
import { useStartJob } from "../hooks";
import type { UpdateStatus } from "../types";

/** An uploaded file that was never looked at (the page closed before `upload_done`). */
function Unchecked({ busy }: { busy: boolean }) {
  const qc = useQueryClient();
  const started = useStartJob();
  const inspect = useMutation({
    mutationFn: uploadDone,
    onSuccess: started("检查上传的文件"),
    onError: (e) => toast.error(errorMessage(e)),
  });
  const cancel = useMutation({
    mutationFn: uploadCancel,
    onSuccess: () => toast.success("已删除上传的文件"),
    onError: (e) => toast.error(errorMessage(e)),
    onSettled: () => qc.invalidateQueries({ queryKey: UPDATE_KEY }),
  });
  return (
    <Alert>
      <AlertTitle>设备上有一个尚未检查的上传文件</AlertTitle>
      <AlertDescription className="flex flex-col items-start gap-2">
        检查后才能安装；如不需要可删除。
        <div className="flex flex-wrap gap-2">
          <Button
            size="sm"
            disabled={busy || inspect.isPending || cancel.isPending}
            aria-busy={inspect.isPending}
            onClick={() => inspect.mutate()}
          >
            {inspect.isPending ? <SweepShine>提交中</SweepShine> : "检查文件"}
          </Button>
          <ConfirmButton
            size="sm"
            variant="outline"
            pending={cancel.isPending}
            disabled={busy || inspect.isPending}
            title="删除上传的文件？"
            description="删除后需要重新上传。"
            confirmText="删除"
            onConfirm={() => cancel.mutate()}
          >
            删除
          </ConfirmButton>
        </div>
      </AlertDescription>
    </Alert>
  );
}

export function OfflineCard({
  status: s,
  busy,
}: {
  status: UpdateStatus;
  busy: boolean;
}) {
  const { present, inspect } = s.upload;
  return (
    <Card size="sm">
      <CardHeader>
        <CardTitle>离线更新</CardTitle>
        <CardDescription>
          设备无法联网时，上传从发布页下载的更新包进行安装
        </CardDescription>
      </CardHeader>
      <CardContent className="flex flex-col gap-4">
        {present && inspect && (
          <InspectResult inspected={inspect} busy={busy} />
        )}
        {present && !inspect && <Unchecked busy={busy} />}
        <UploadForm busy={busy} />
      </CardContent>
    </Card>
  );
}
