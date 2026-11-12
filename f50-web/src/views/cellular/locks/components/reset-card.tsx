import { ConfirmButton } from "@/components/f50/confirm-button";
import { KvList } from "@/components/f50/kv-list";
import { Badge } from "@/components/ui/badge";
import {
  Card,
  CardContent,
  CardDescription,
  CardFooter,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import { reportFormError } from "@/lib/form-error";
import { formatAgo, formatTime } from "@/lib/format";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { lockKeys, resetLocks } from "../api";
import type { ResetResult } from "../types";
import type { OnJob } from "../use-lock-set";

const STATE: Record<
  ResetResult["state"],
  { label: string; variant: "secondary" | "destructive" | "outline" }
> = {
  running: { label: "进行中", variant: "outline" },
  ok: { label: "成功", variant: "secondary" },
  failed: { label: "失败", variant: "destructive" },
};

export function ResetCard({
  last,
  disabled,
  onJob,
}: {
  last: ResetResult | null;
  disabled: boolean;
  onJob: OnJob;
}) {
  const qc = useQueryClient();
  const reset = useMutation({
    mutationFn: resetLocks,
    onSuccess: (r) => {
      onJob(r.job, "全部恢复自动");
      qc.invalidateQueries({ queryKey: lockKeys.get });
    },
    onError: (e) => reportFormError(e),
  });
  return (
    <Card>
      <CardHeader>
        <CardTitle>全部恢复自动</CardTitle>
        <CardDescription>
          清除网络模式、频段、小区与 EN-DC 锁定并逐项校验；已保存的锁定一并删除
        </CardDescription>
      </CardHeader>
      <CardContent>
        <KvList
          items={[
            {
              label: "上次重置",
              value: last ? (
                <Badge variant={STATE[last.state].variant}>
                  {STATE[last.state].label}
                </Badge>
              ) : (
                "无记录"
              ),
            },
            ...(last
              ? [
                  {
                    label: "时间",
                    value: formatAgo(last.ts),
                    title: formatTime(last.ts),
                  },
                  ...(last.error
                    ? [
                        {
                          label: "错误",
                          value: `${last.error}${last.step ? `（${last.step}）` : ""}`,
                        },
                      ]
                    : []),
                ]
              : []),
          ]}
        />
      </CardContent>
      <CardFooter>
        <ConfirmButton
          title="全部锁定恢复自动？"
          description="所有锁定将被清除并校验，协议栈会重启，蜂窝连接中断约半分钟。"
          confirmText="恢复自动"
          pending={reset.isPending}
          disabled={disabled}
          onConfirm={() => reset.mutate()}
        >
          恢复自动
        </ConfirmButton>
      </CardFooter>
    </Card>
  );
}
