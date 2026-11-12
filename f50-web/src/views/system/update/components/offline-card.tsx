import { ConfirmButton } from "@/components/f50/confirm-button";
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert";
import { Button } from "@/components/ui/button";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import { SweepShine } from "@/components/ui/sweep-shine";
import { t } from "@/i18n";
import { errorMessage } from "@/lib/format";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { toast } from "sonner";
import { InspectResult } from "./inspect-result";
import { UploadForm } from "./upload-form";
import { UPDATE_KEY, uploadCancel, uploadDone } from "../api";
import { useStartJob } from "../hooks";
import type { UpdateStatus } from "../types";

/** An uploaded file that was never looked at (the page closed before `upload_done`). */
function Unchecked({ busy }: { busy: boolean }) {
  const qc = useQueryClient();
  const started = useStartJob();
  const inspect = useMutation({
    mutationFn: uploadDone,
    onSuccess: started(t("maintenance.update.jobTitle.uploadDone")),
    onError: (e) => toast.error(errorMessage(e)),
  });
  const cancel = useMutation({
    mutationFn: uploadCancel,
    onSuccess: () => toast.success(t("maintenance.update.offline.deleted")),
    onError: (e) => toast.error(errorMessage(e)),
    onSettled: () => qc.invalidateQueries({ queryKey: UPDATE_KEY }),
  });
  return (
    <Alert>
      <AlertTitle>{t("maintenance.update.offline.uncheckedTitle")}</AlertTitle>
      <AlertDescription className="flex flex-col items-start gap-2">
        {t("maintenance.update.offline.uncheckedBody")}
        <div className="flex flex-wrap gap-2">
          <Button
            size="sm"
            disabled={busy || inspect.isPending || cancel.isPending}
            aria-busy={inspect.isPending}
            onClick={() => inspect.mutate()}
          >
            {inspect.isPending ? (
              <SweepShine>{t("maintenance.update.submitting")}</SweepShine>
            ) : (
              t("maintenance.update.offline.inspect")
            )}
          </Button>
          <ConfirmButton
            size="sm"
            variant="outline"
            pending={cancel.isPending}
            disabled={busy || inspect.isPending}
            title={t("maintenance.update.offline.deleteTitle")}
            description={t("maintenance.update.offline.deleteBody")}
            confirmText={t("common.action.delete")}
            onConfirm={() => cancel.mutate()}
          >
            {t("common.action.delete")}
          </ConfirmButton>
        </div>
      </AlertDescription>
    </Alert>
  );
}

export function OfflineCard({
  status: s,
  busy,
}: {
  status: UpdateStatus;
  busy: boolean;
}) {
  const { present, inspect } = s.upload;
  return (
    <Card size="sm">
      <CardHeader>
        <CardTitle>{t("maintenance.update.offline.title")}</CardTitle>
        <CardDescription>
          {t("maintenance.update.offline.description")}
        </CardDescription>
      </CardHeader>
      <CardContent className="flex flex-col gap-4">
        {present && inspect && (
          <InspectResult inspected={inspect} busy={busy} />
        )}
        {present && !inspect && <Unchecked busy={busy} />}
        <UploadForm busy={busy} />
      </CardContent>
    </Card>
  );
}
