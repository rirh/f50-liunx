import { KvList } from "@/components/f50/kv-list";
import { Badge } from "@/components/ui/badge";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import { Progress } from "@/components/ui/progress";
import { formatBytes } from "@/lib/format";
import { SystemsTable } from "./systems-table";
import type { UpdateStatus } from "../types";

const KEY_LABEL = {
  ok: "已安装",
  none: "未安装（无法校验签名）",
  unsafe: "不安全（已拒绝使用）",
} as const;
const FALLBACK_LABEL = {
  auto: "自动回退 GitHub",
  never: "从不回退",
  always: "总是使用 GitHub",
} as const;

function Disk({ disk }: { disk: UpdateStatus["disk"] }) {
  if (disk.free_kb == null || disk.total_kb == null) return <>—</>;
  const used = ((disk.total_kb - disk.free_kb) / disk.total_kb) * 100;
  const enough = disk.free_kb >= disk.need_all_kb;
  return (
    <div className="flex flex-col gap-1">
      <span className="flex flex-wrap items-center gap-2">
        可用 {formatBytes(disk.free_kb * 1024)} / 共{" "}
        {formatBytes(disk.total_kb * 1024)}
        {!enough && (
          <Badge variant="destructive">
            不足 {formatBytes(disk.need_all_kb * 1024)}
          </Badge>
        )}
      </span>
      <Progress value={used} aria-label="Linux 分区已用空间" />
    </div>
  );
}

export function StatusCard({ status: s }: { status: UpdateStatus }) {
  return (
    <Card size="sm">
      <CardHeader>
        <CardTitle>已安装</CardTitle>
        <CardDescription>
          当前运行的版本、内核与启动镜像，以及可用于回滚的备份
        </CardDescription>
      </CardHeader>
      <CardContent className="flex flex-col gap-4">
        <KvList
          items={[
            {
              label: "当前版本",
              value: <span className="font-mono">{s.installed ?? "未知"}</span>,
            },
            {
              label: "运行内核",
              value: (
                <span className="font-mono">{s.kernel.running ?? "—"}</span>
              ),
            },
            {
              label: "内核系列",
              value: `${s.kernel.choice}（可选 ${s.kernel.choices.join(" / ")}）`,
            },
            {
              label: "启动镜像",
              value: (
                <span className="font-mono">
                  {s.boot.release ?? "—"} · {s.boot.kernel ?? "—"} · 槽位{" "}
                  {s.boot.slot}
                </span>
              ),
            },
            {
              label: "启动镜像备份",
              value: s.boot.previous ? (
                <span className="flex flex-wrap items-center gap-2 font-mono">
                  {s.boot.previous.release ?? "未知版本"} ·{" "}
                  {s.boot.previous.kernel ?? "—"}
                  {!s.boot.previous.usable && (
                    <Badge variant="outline">槽位不符，不可回滚</Badge>
                  )}
                </span>
              ) : (
                "无"
              ),
            },
            {
              label: "默认启动",
              value: s.boot.default === "linux" ? "Linux" : "Android",
            },
            { label: "Linux 分区", value: <Disk disk={s.disk} /> },
            {
              label: "发布密钥",
              value: KEY_LABEL[s.signing.key] ?? s.signing.key,
            },
            {
              label: "更新源",
              value: s.source.mirror
                ? `镜像（${FALLBACK_LABEL[s.source.fallback] ?? s.source.fallback}${s.source.fell_back ? "，本次已回退" : ""}）`
                : "GitHub",
            },
          ]}
        />
        <SystemsTable systems={s.systems} />
      </CardContent>
    </Card>
  );
}
