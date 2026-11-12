import { Badge } from "@/components/ui/badge";
import { formatAgo, formatTime } from "@/lib/format";
import { HardDriveDownloadIcon } from "lucide-react";
import { TextLink } from "./text-link";
import { Figure, Tile } from "./tile";
import type { Part, UpdateBrief } from "../types";

export function UpdateTile({ part }: { part: Part<UpdateBrief> | undefined }) {
  return (
    <Tile
      title="系统更新"
      icon={HardDriveDownloadIcon}
      to="/system/update"
      part={part}
    >
      {(u) => (
        <>
          <div className="flex items-center justify-between gap-3">
            <Figure value={u.installed ?? "未知版本"} caption="当前版本" />
            {u.job ? (
              <Badge>更新任务进行中</Badge>
            ) : u.notice ? (
              <Badge>有新版本</Badge>
            ) : (
              <Badge variant="secondary">已是最新</Badge>
            )}
          </div>
          {u.restart_pending && (
            <Badge variant="outline">更新已安装，等待重启</Badge>
          )}
          {u.notice && (
            <p className="text-sm" title={formatTime(u.notice.at)}>
              新版本 <span className="font-mono">{u.notice.tag}</span>（
              {formatAgo(u.notice.at)}发现）
            </p>
          )}
          {(u.notice || u.job) && (
            <TextLink
              to="/system/update"
              className="self-start text-sm text-primary"
            >
              {u.job ? "查看更新进度" : "前往更新"}
            </TextLink>
          )}
        </>
      )}
    </Tile>
  );
}
