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
import { type Key, t } from "@/i18n";
import { errorMessage } from "@/lib/format";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { toast } from "sonner";
import { cpuKey, setProfile } from "../api";
import type { CpuProfile, CpuStatus } from "../types";

const PROFILE_TEXT: Record<CpuProfile, { label: Key; description: Key }> = {
  saving: {
    label: "system.cpu.profile.label.saving",
    description: "system.cpu.profile.text.saving",
  },
  balanced: {
    label: "system.cpu.profile.label.balanced",
    description: "system.cpu.profile.text.balanced",
  },
  performance: {
    label: "system.cpu.profile.label.performance",
    description: "system.cpu.profile.text.performance",
  },
};

export function ProfileCard({ data }: { data: CpuStatus }) {
  const qc = useQueryClient();
  const apply = useMutation({
    mutationFn: setProfile,
    onSuccess: (r) => {
      qc.setQueryData(cpuKey, r);
      toast.success(
        t("system.cpu.profile.switched", {
          profile: t(PROFILE_TEXT[r.profile].label),
        }),
      );
    },
    onError: (e) => toast.error(errorMessage(e)),
    onSettled: () => qc.invalidateQueries({ queryKey: cpuKey }),
  });
  const shown =
    apply.isPending && apply.variables ? apply.variables : data.profile;
  return (
    <Card>
      <CardHeader>
        <CardTitle>{t("system.cpu.profile.title")}</CardTitle>
        <CardDescription>
          {data.saved
            ? t("system.cpu.profile.saved")
            : t("system.cpu.profile.unsaved")}
        </CardDescription>
        <CardAction>
          {data.throttling ? (
            <Badge variant="destructive">
              {t("system.cpu.profile.throttling")}
            </Badge>
          ) : (
            <Badge variant="outline">
              {t("system.cpu.profile.notThrottling")}
            </Badge>
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
                <SweepShine>{t(PROFILE_TEXT[p].label)}</SweepShine>
              ) : (
                t(PROFILE_TEXT[p].label)
              )}
            </ToggleGroupItem>
          ))}
        </ToggleGroup>
        <dl className="grid gap-2 text-sm sm:grid-cols-3">
          {data.profiles.map((p) => (
            <div key={p} className="flex flex-col gap-0.5">
              <dt className="font-medium">{t(PROFILE_TEXT[p].label)}</dt>
              <dd className="text-muted-foreground">
                {t(PROFILE_TEXT[p].description)}
              </dd>
            </div>
          ))}
        </dl>
      </CardContent>
    </Card>
  );
}
