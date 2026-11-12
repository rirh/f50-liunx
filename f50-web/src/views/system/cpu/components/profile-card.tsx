import { Badge } from "@/components/ui/badge";
import {
  Card,
  CardAction,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import { SweepShine } from "@/components/ui/sweep-shine";
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group";
import { errorMessage } from "@/lib/format";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { toast } from "sonner";
import { cpuKey, setProfile } from "../api";
import type { CpuProfile, CpuStatus } from "../types";

const PROFILE_TEXT: Record<CpuProfile, { label: string; description: string }> =
  {
    saving: {
      label: "省电",
      description:
        "schedutil 调度，最高频率限制在硬件最高频率约 60% 以内的档位，发热和功耗最低。",
    },
    balanced: {
      label: "均衡",
      description: "schedutil 调度，按负载升降频，不限制最高频率。默认配置。",
    },
    performance: {
      label: "性能",
      description: "performance 调度，始终运行在最高频率，响应最快但发热最大。",
    },
  };

export function ProfileCard({ data }: { data: CpuStatus }) {
  const qc = useQueryClient();
  const apply = useMutation({
    mutationFn: setProfile,
    onSuccess: (r) => {
      qc.setQueryData(cpuKey, r);
      toast.success(`已切换到${PROFILE_TEXT[r.profile].label}模式`);
    },
    onError: (e) => toast.error(errorMessage(e)),
    onSettled: () => qc.invalidateQueries({ queryKey: cpuKey }),
  });
  const shown =
    apply.isPending && apply.variables ? apply.variables : data.profile;
  return (
    <Card>
      <CardHeader>
        <CardTitle>CPU 性能模式</CardTitle>
        <CardDescription>
          {data.saved
            ? "已保存，开机时自动应用"
            : "未保存过，使用默认的均衡模式"}
        </CardDescription>
        <CardAction>
          {data.throttling ? (
            <Badge variant="destructive">正在限频</Badge>
          ) : (
            <Badge variant="outline">未限频</Badge>
          )}
        </CardAction>
      </CardHeader>
      <CardContent className="flex flex-col gap-3">
        <ToggleGroup
          type="single"
          variant="outline"
          spacing={0}
          value={shown}
          disabled={apply.isPending}
          aria-busy={apply.isPending}
          onValueChange={(v) => {
            if (v && v !== data.profile) apply.mutate(v as CpuProfile);
          }}
          className="w-full sm:w-fit"
        >
          {data.profiles.map((p) => (
            <ToggleGroupItem
              key={p}
              value={p}
              className="flex-1 sm:flex-none sm:px-6"
            >
              {apply.isPending && apply.variables === p ? (
                <SweepShine>{PROFILE_TEXT[p].label}</SweepShine>
              ) : (
                PROFILE_TEXT[p].label
              )}
            </ToggleGroupItem>
          ))}
        </ToggleGroup>
        <dl className="grid gap-2 text-sm sm:grid-cols-3">
          {data.profiles.map((p) => (
            <div key={p} className="flex flex-col gap-0.5">
              <dt className="font-medium">{PROFILE_TEXT[p].label}</dt>
              <dd className="text-muted-foreground">
                {PROFILE_TEXT[p].description}
              </dd>
            </div>
          ))}
        </dl>
      </CardContent>
    </Card>
  );
}
