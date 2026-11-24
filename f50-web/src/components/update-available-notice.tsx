import * as React from "react";
import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import { SweepShine } from "@/components/ui/sweep-shine";
import { t } from "@/i18n";
import { updatePendingAtom } from "@/store/app-update";
import { useAtom } from "jotai";
import { X } from "lucide-react";

export interface UpdateAvailableNoticeProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  onUpdate?: () => void | Promise<void>;
  /** 稍后提醒: shown when given, beside the update button. */
  onLater?: () => void;
  title?: string;
  description?: string;
  updateLabel?: string;
  updatingLabel?: string;
  closeLabel?: string;
  resetUpdatingAfterUpdate?: boolean;
}

export function UpdateAvailableNotice({
  open,
  onOpenChange,
  onUpdate,
  onLater,
  title,
  description,
  updateLabel,
  updatingLabel,
  closeLabel,
  resetUpdatingAfterUpdate = false,
}: UpdateAvailableNoticeProps) {
  title ??= t("app.pageUpdate.title");
  description ??= t("app.pageUpdate.description");
  updateLabel ??= t("app.pageUpdate.action");
  updatingLabel ??= t("app.pageUpdate.running");
  closeLabel ??= t("app.pageUpdate.close");
  const [updating, setUpdating] = useAtom(updatePendingAtom);
  const [error, setError] = React.useState<string | null>(null);

  async function handleUpdate() {
    if (updating) return;
    setError(null);
    setUpdating(true);
    try {
      await onUpdate?.();
    } catch {
      setError(t("app.pageUpdate.failed"));
      setUpdating(false);
    } finally {
      if (resetUpdatingAfterUpdate) {
        setUpdating(false);
      }
    }
  }

  if (!open) return null;

  return (
    <div>
      <Card
        role="alert"
        aria-live="polite"
        aria-atomic="true"
        aria-label={title}
        className="animate-in fade-in slide-in-from-bottom-3 bg-card gap-0 rounded-lg border-0 p-3 shadow-sm ring-0 duration-300 motion-reduce:animate-none"
      >
        <div className="flex items-start gap-2">
          <div className="min-w-0 flex-1">
            <p className="text-sm leading-5 font-medium">{title}</p>
            <p className="text-muted-foreground mt-1 text-xs leading-4">
              {error ?? description}
            </p>
          </div>
          <Button
            type="button"
            size="icon-xs"
            variant="ghost"
            className="text-muted-foreground/60 hover:text-muted-foreground -me-1 -mt-1 shrink-0"
            onClick={() => onOpenChange(false)}
            disabled={updating}
            aria-label={closeLabel}
          >
            <X aria-hidden="true" />
          </Button>
        </div>
        <div className="mt-3 flex justify-end gap-2">
          {onLater && (
            <Button
              type="button"
              size="sm"
              variant="outline"
              onClick={onLater}
              disabled={updating}
              title={t("app.pageUpdate.laterHint")}
            >
              {t("app.pageUpdate.later")}
            </Button>
          )}
          <Button
            type="button"
            size="sm"
            className="min-w-20"
            onClick={() => void handleUpdate()}
            disabled={updating}
            aria-busy={updating || undefined}
          >
            <SweepShine
              active={updating}
              className={updating ? "text-primary-foreground/70" : undefined}
            >
              {updating ? updatingLabel : updateLabel}
            </SweepShine>
          </Button>
        </div>
      </Card>
    </div>
  );
}
