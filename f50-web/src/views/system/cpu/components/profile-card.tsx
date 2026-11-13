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
import { type Key, t } from "@/i18n";
import { errorMessage } from "@/lib/format";
import { cn } from "@/lib/utils";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { FlameIcon, GaugeIcon, LeafIcon, type LucideIcon } from "lucide-react";
import { toast } from "sonner";
import { cpuKey, setProfile } from "../api";
import type { CpuProfile, CpuStatus } from "../types";

const PROFILE_TEXT: Record<
  CpuProfile,
  { label: Key; short: Key; description: Key; icon: LucideIcon }
> = {
  saving: {
    label: "system.cpu.profile.label.saving",
    short: "system.cpu.profile.short.saving",
    description: "system.cpu.profile.text.saving",
    icon: LeafIcon,
  },
  balanced: {
    label: "system.cpu.profile.label.balanced",
    short: "system.cpu.profile.short.balanced",
    description: "system.cpu.profile.text.balanced",
    icon: GaugeIcon,
  },
  performance: {
    label: "system.cpu.profile.label.performance",
    short: "system.cpu.profile.short.performance",
    description: "system.cpu.profile.text.performance",
    icon: FlameIcon,
  },
};

/**
 * The mode as three cards to pick from: name and one plain line each, the chosen one marked. Picking applies at once.
 * The governor details stay in each card's tooltip for those who want them.
 */

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
      <CardContent>
        <div
          role="radiogroup"
          aria-label={t("system.cpu.profile.title")}
          aria-busy={apply.isPending}
          className="grid gap-2 sm:grid-cols-3"
        >
          {data.profiles.map((p) => {
            const text = PROFILE_TEXT[p];
            const Icon = text.icon;
            const on = shown === p;
            const busy = apply.isPending && apply.variables === p;
            return (
              <button
                key={p}
                type="button"
                role="radio"
                aria-checked={on}
                title={t(text.description)}
                disabled={apply.isPending}
                onClick={() => p !== data.profile && apply.mutate(p)}
                className={cn(
                  "flex items-start gap-3 rounded-xl border p-3 text-left transition-colors outline-none focus-visible:ring-3 focus-visible:ring-ring/50 disabled:cursor-default",
                  on
                    ? "border-primary bg-primary/6 ring-1 ring-primary"
                    : "hover:bg-muted/60",
                )}
              >
                <span
                  className={cn(
                    "flex size-8 shrink-0 items-center justify-center rounded-lg",
                    on
                      ? "bg-primary text-primary-foreground"
                      : "bg-muted text-muted-foreground",
                  )}
                >
                  <Icon aria-hidden className="size-4" />
                </span>
                <span className="flex min-w-0 flex-col gap-0.5">
                  <span className="text-sm font-medium">
                    {busy ? (
                      <SweepShine>{t(text.label)}</SweepShine>
                    ) : (
                      t(text.label)
                    )}
                  </span>
                  <span className="text-xs text-muted-foreground">
                    {t(text.short)}
                  </span>
                </span>
              </button>
            );
          })}
        </div>
      </CardContent>
    </Card>
  );
}
