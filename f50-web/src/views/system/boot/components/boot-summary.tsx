import { KvList } from "@/components/f50/kv-list";
import { Badge } from "@/components/ui/badge";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import type { BootInfo } from "../types";

const yes = (b: boolean, on: string, off: string) => (
  <Badge variant={b ? "secondary" : "outline"}>{b ? on : off}</Badge>
);

export function BootSummary({ info }: { info: BootInfo }) {
  const linuxDefault = info.default_boot === "linux";
  return (
    <Card>
      <CardHeader>
        <CardTitle>启动控制</CardTitle>
        <CardDescription>
          决定设备保持启动 Linux 还是回退到 Android（只读）
        </CardDescription>
      </CardHeader>
      <CardContent>
        <KvList
          items={[
            {
              label: "Linux 所在槽位",
              value: (
                <span className="font-mono">{info.slot.toUpperCase()}</span>
              ),
            },
            {
              label: "默认启动",
              value: (
                <span className="flex flex-wrap items-center gap-2">
                  <Badge variant={linuxDefault ? "secondary" : "outline"}>
                    {linuxDefault ? "Linux" : "Android"}
                  </Badge>
                  <span className="text-muted-foreground">
                    {linuxDefault
                      ? "每次正常启动后重新武装 Linux"
                      : "每次 Linux 启动都只是一次性尝试"}
                  </span>
                </span>
              ),
            },
            {
              label: "允许失败次数",
              value: `连续 ${info.attempts} 次启动失败后回退到 Android`,
            },
            {
              label: "锁定 Linux",
              value: (
                <span className="flex flex-wrap items-center gap-2">
                  {yes(info.locked, "已锁定", "未锁定")}
                  {info.locked && (
                    <span className="text-muted-foreground">
                      无论启动结果如何都保持 Linux
                    </span>
                  )}
                </span>
              ),
            },
            {
              label: "上次成功后的启动次数",
              value:
                info.boots_since_good == null
                  ? "—"
                  : String(info.boots_since_good),
            },
            {
              label: "启动控制块可写",
              value: yes(info.writable, "可写", "不可写"),
            },
          ]}
        />
      </CardContent>
    </Card>
  );
}
