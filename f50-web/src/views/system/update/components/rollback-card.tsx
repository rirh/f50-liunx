import { ConfirmButton } from "@/components/f50/confirm-button";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import { errorMessage } from "@/lib/format";
import { useMutation } from "@tanstack/react-query";
import { toast } from "sonner";
import { rollback } from "../api";
import { useStartJob } from "../hooks";
import type { RollbackWhat, UpdateStatus } from "../types";

const WARNING =
  "回滚开始后无法停止，期间请保持电源连接，不要重启或拔出设备。完成后需重启生效。";

export function RollbackCard({
  status: s,
  busy,
}: {
  status: UpdateStatus;
  busy: boolean;
}) {
  const started = useStartJob();
  const run = useMutation({
    mutationFn: (what: RollbackWhat) => rollback(what),
    onSuccess: (r, what) =>
      started(what === "boot" ? "回滚启动镜像" : "回滚系统")(r),
    onError: (e) => toast.error(errorMessage(e)),
  });
  const kept = s.systems.filter((x) => x.previous);
  const bootPrev = s.boot.previous;

  return (
    <Card size="sm">
      <CardHeader>
        <CardTitle>回滚</CardTitle>
        <CardDescription>恢复到更新前保留的系统或启动镜像</CardDescription>
      </CardHeader>
      <CardContent className="flex flex-wrap gap-2">
        <ConfirmButton
          size="sm"
          variant="outline"
          pending={run.isPending && run.variables === "system"}
          disabled={busy || run.isPending || kept.length === 0}
          title="回滚系统？"
          confirmText="回滚系统"
          description={
            <span className="flex flex-col gap-2">
              <span>
                将恢复：
                {kept
                  .map(
                    (x) => `${x.name} → ${x.previous?.release ?? "未知版本"}`,
                  )
                  .join("；") || "无"}
              </span>
              <span className="font-medium text-destructive">{WARNING}</span>
            </span>
          }
          onConfirm={() => run.mutate("system")}
        >
          {kept.length ? "回滚系统" : "无系统备份"}
        </ConfirmButton>
        <ConfirmButton
          size="sm"
          variant="outline"
          pending={run.isPending && run.variables === "boot"}
          disabled={busy || run.isPending || !bootPrev?.usable}
          title="回滚启动镜像？"
          confirmText="回滚启动镜像"
          description={
            <span className="flex flex-col gap-2">
              <span>
                将恢复启动镜像 {bootPrev?.release ?? "未知版本"}（内核{" "}
                {bootPrev?.kernel ?? "—"}）。
              </span>
              <span className="font-medium text-destructive">{WARNING}</span>
            </span>
          }
          onConfirm={() => run.mutate("boot")}
        >
          {bootPrev?.usable ? "回滚启动镜像" : "无可用启动镜像备份"}
        </ConfirmButton>
      </CardContent>
    </Card>
  );
}
