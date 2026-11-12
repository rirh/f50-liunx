import { ConfirmButton } from "@/components/f50/confirm-button";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
} from "@/components/ui/collapsible";
import { errorMessage, formatBytes } from "@/lib/format";
import { useMutation } from "@tanstack/react-query";
import { ChevronDownIcon } from "lucide-react";
import { toast } from "sonner";
import { apply } from "../api";
import { useStartJob } from "../hooks";
import type { Offer, UpdateStatus } from "../types";

/** One release the last check offered, and its install button (not cancellable once it starts). */
export function OfferItem({
  offer,
  status,
  busy,
}: {
  offer: Offer;
  status: UpdateStatus;
  busy: boolean;
}) {
  const started = useStartJob();
  const install = useMutation({
    mutationFn: () => apply(offer.tag),
    onSuccess: started(`安装 ${offer.tag}`),
    onError: (e) => toast.error(errorMessage(e)),
  });
  const current = offer.systems.length === 0;
  const lowSpace =
    status.disk.free_kb != null &&
    status.disk.free_kb < status.disk.need_all_kb;

  return (
    <div className="flex flex-col gap-2 rounded-lg border p-3">
      <div className="flex flex-wrap items-start justify-between gap-2">
        <div className="flex min-w-0 flex-col gap-1">
          <span className="flex flex-wrap items-center gap-2">
            <span className="font-mono font-medium">{offer.tag}</span>
            {offer.prerelease && <Badge variant="outline">测试版</Badge>}
            <Badge variant={offer.signed ? "secondary" : "destructive"}>
              {offer.signed ? "已签名" : "未签名"}
            </Badge>
            {current && <Badge variant="secondary">已安装</Badge>}
          </span>
          <span className="text-sm text-muted-foreground">
            {offer.title ?? "无标题"}
            {offer.published && ` · 发布于 ${offer.published}`}
          </span>
          <span className="text-sm text-muted-foreground">
            {current
              ? "所有系统已是此版本"
              : `将更新：${offer.systems.join("、")}`}{" "}
            · 下载 {formatBytes(offer.download)}
          </span>
        </div>
        <ConfirmButton
          size="sm"
          variant={current ? "outline" : "default"}
          destructive={false}
          pending={install.isPending}
          disabled={busy}
          title={`安装 ${offer.tag}？`}
          confirmText="开始安装"
          description={
            <span className="flex flex-col gap-2">
              <span>
                将下载约 {formatBytes(offer.download)} 并写入
                {current ? "系统" : `：${offer.systems.join("、")}`}
                ，完成后需重启生效。旧系统会保留一份用于回滚。
              </span>
              <span className="font-medium text-destructive">
                安装开始后无法停止。期间请保持电源连接，不要重启或拔出设备，否则可能导致无法启动。
              </span>
              {lowSpace && (
                <span className="text-destructive">
                  Linux 分区可用空间可能不足。
                </span>
              )}
            </span>
          }
          onConfirm={() => install.mutate()}
        >
          {install.isPending ? "启动中" : current ? "重新安装" : "安装"}
        </ConfirmButton>
      </div>
      {offer.files.length > 0 && (
        <Collapsible>
          <CollapsibleTrigger asChild>
            <Button variant="ghost" size="sm" className="self-start">
              <ChevronDownIcon data-icon="inline-start" />
              {offer.files.length} 个文件
            </Button>
          </CollapsibleTrigger>
          <CollapsibleContent>
            <ul className="flex flex-col gap-1 text-xs">
              {offer.files.map((f) => (
                <li key={f.name} className="flex justify-between gap-2">
                  <span className="truncate font-mono">{f.name}</span>
                  <span className="text-muted-foreground tabular-nums">
                    {formatBytes(f.size)}
                  </span>
                </li>
              ))}
            </ul>
          </CollapsibleContent>
        </Collapsible>
      )}
    </div>
  );
}
