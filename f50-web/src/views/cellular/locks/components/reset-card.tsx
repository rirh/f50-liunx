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
import { t, type Key } from "@/i18n";
import { reportFormError } from "@/lib/form-error";
import { formatAgo, formatTime } from "@/lib/format";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { lockKeys, resetLocks } from "../api";
import type { ResetResult } from "../types";
import type { OnJob } from "../use-lock-set";

const STATE: Record<
  ResetResult["state"],
  { label: Key; variant: "secondary" | "destructive" | "outline" }
> = {
  running: { label: "cellular.locks.reset.state.running", variant: "outline" },
  ok: { label: "cellular.locks.reset.state.ok", variant: "secondary" },
  failed: {
    label: "cellular.locks.reset.state.failed",
    variant: "destructive",
  },
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
      onJob(r.job, t("cellular.locks.reset.title"));
      qc.invalidateQueries({ queryKey: lockKeys.get });
    },
    onError: (e) => reportFormError(e),
  });
  return (
    <Card>
      <CardHeader>
        <CardTitle>{t("cellular.locks.reset.title")}</CardTitle>
        <CardDescription>
          {t("cellular.locks.reset.description")}
        </CardDescription>
      </CardHeader>
      <CardContent>
        <KvList
          items={[
            {
              label: t("cellular.locks.reset.last"),
              value: last ? (
                <Badge variant={STATE[last.state].variant}>
                  {t(STATE[last.state].label)}
                </Badge>
              ) : (
                t("cellular.locks.reset.none")
              ),
            },
            ...(last
              ? [
                  {
                    label: t("cellular.locks.reset.time"),
                    value: formatAgo(last.ts),
                    title: formatTime(last.ts),
                  },
                  ...(last.error
                    ? [
                        {
                          label: t("cellular.locks.reset.error"),
                          value: `${last.error}${last.step ? t("cellular.locks.reset.step", { step: last.step }) : ""}`,
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
          title={t("cellular.locks.reset.confirmTitle")}
          description={t("cellular.locks.reset.confirmDescription")}
          confirmText={t("cellular.locks.reset.action")}
          pending={reset.isPending}
          disabled={disabled}
          onConfirm={() => reset.mutate()}
        >
          {t("cellular.locks.reset.action")}
        </ConfirmButton>
      </CardFooter>
    </Card>
  );
}
