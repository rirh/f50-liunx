import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert";
import { formatTime } from "@/lib/format";
import { InfoIcon, TriangleAlertIcon } from "lucide-react";
import type { UpdateStatus } from "../types";

/** What needs attention: a pending restart, a half-done run, what recovery did, a lock held elsewhere. */
export function StatusAlerts({ status: s }: { status: UpdateStatus }) {
  return (
    <>
      {s.restart_pending && (
        <Alert>
          <InfoIcon />
          <AlertTitle>更新已写入，重启后生效</AlertTitle>
          <AlertDescription>
            当前仍在运行旧系统（或回滚前的系统）。请在方便时重启设备。
          </AlertDescription>
        </Alert>
      )}
      {s.unfinished && (
        <Alert variant="destructive">
          <TriangleAlertIcon />
          <AlertTitle>上次更新未完成</AlertTitle>
          <AlertDescription>
            更新程序会在下次运行时自动恢复或清理；如持续出现，请查看系统日志。
          </AlertDescription>
        </Alert>
      )}
      {s.recovery && s.recovery.lines.length > 0 && (
        <Alert>
          <InfoIcon />
          <AlertTitle>
            重启后的自动恢复（{formatTime(s.recovery.at)}）
          </AlertTitle>
          <AlertDescription>
            <ul className="flex list-disc flex-col gap-1 pl-4">
              {s.recovery.lines.map((l, i) => (
                <li key={i}>{l}</li>
              ))}
            </ul>
          </AlertDescription>
        </Alert>
      )}
      {s.lock && !s.lock.this_daemon && (
        <Alert variant="destructive">
          <TriangleAlertIcon />
          <AlertTitle>另一个更新程序正在运行</AlertTitle>
          <AlertDescription>
            进程 {s.lock.pid}
            {s.lock.command ? `（${s.lock.command}）` : ""}{" "}
            持有更新锁，结束前无法在此操作。
          </AlertDescription>
        </Alert>
      )}
    </>
  );
}
