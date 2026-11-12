import { ConfirmButton } from "@/components/f50/confirm-button";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
} from "@/components/ui/collapsible";
import { joinList, t } from "@/i18n";
import { errorMessage, formatBytes } from "@/lib/format";
import { useMutation } from "@tanstack/react-query";
import { ChevronDownIcon } from "lucide-react";
import { toast } from "sonner";
import { apply } from "../api";
import { useStartJob } from "../hooks";
import type { Offer, UpdateStatus } from "../types";

/** One release the last check offered, and its install button (not cancellable once it starts). */
export function OfferItem({
  offer,
  status,
  busy,
}: {
  offer: Offer;
  status: UpdateStatus;
  busy: boolean;
}) {
  const started = useStartJob();
  const install = useMutation({
    mutationFn: () => apply(offer.tag),
    onSuccess: started(
      t("maintenance.update.offer.jobTitle", { tag: offer.tag }),
    ),
    onError: (e) => toast.error(errorMessage(e)),
  });
  const current = offer.systems.length === 0;
  const lowSpace =
    status.disk.free_kb != null &&
    status.disk.free_kb < status.disk.need_all_kb;

  return (
    <div className="flex flex-col gap-2 rounded-lg border p-3">
      <div className="flex flex-wrap items-start justify-between gap-2">
        <div className="flex min-w-0 flex-col gap-1">
          <span className="flex flex-wrap items-center gap-2">
            <span className="font-mono font-medium">{offer.tag}</span>
            {offer.prerelease && (
              <Badge variant="outline">
                {t("maintenance.update.offer.prerelease")}
              </Badge>
            )}
            <Badge variant={offer.signed ? "secondary" : "destructive"}>
              {offer.signed
                ? t("maintenance.update.offer.signed")
                : t("maintenance.update.offer.unsigned")}
            </Badge>
            {current && (
              <Badge variant="secondary">
                {t("maintenance.update.offer.installed")}
              </Badge>
            )}
          </span>
          <span className="text-sm text-muted-foreground">
            {offer.title ?? t("maintenance.update.offer.untitled")}
            {offer.published &&
              ` · ${t("maintenance.update.offer.published", { date: offer.published })}`}
          </span>
          <span className="text-sm text-muted-foreground">
            {current
              ? t("maintenance.update.offer.allCurrent")
              : t("maintenance.update.offer.willUpdate", {
                  systems: joinList(offer.systems),
                })}{" "}
            ·{" "}
            {t("maintenance.update.offer.download", {
              size: formatBytes(offer.download),
            })}
          </span>
        </div>
        <ConfirmButton
          size="sm"
          variant={current ? "outline" : "default"}
          destructive={false}
          pending={install.isPending}
          disabled={busy}
          title={t("maintenance.update.offer.confirmTitle", { tag: offer.tag })}
          confirmText={t("maintenance.update.offer.confirmText")}
          description={
            <span className="flex flex-col gap-2">
              <span>
                {current
                  ? t("maintenance.update.offer.confirmBodyAll", {
                      size: formatBytes(offer.download),
                    })
                  : t("maintenance.update.offer.confirmBody", {
                      size: formatBytes(offer.download),
                      systems: joinList(offer.systems),
                    })}
              </span>
              <span className="font-medium text-destructive">
                {t("maintenance.update.offer.noStop")}
              </span>
              {lowSpace && (
                <span className="text-destructive">
                  {t("maintenance.update.offer.lowSpace")}
                </span>
              )}
            </span>
          }
          onConfirm={() => install.mutate()}
        >
          {install.isPending
            ? t("maintenance.update.starting")
            : current
              ? t("maintenance.update.offer.reinstall")
              : t("maintenance.update.offer.install")}
        </ConfirmButton>
      </div>
      {offer.files.length > 0 && (
        <Collapsible>
          <CollapsibleTrigger asChild>
            <Button variant="ghost" size="sm" className="self-start">
              <ChevronDownIcon data-icon="inline-start" />
              {t("maintenance.update.offer.files", {
                count: offer.files.length,
              })}
            </Button>
          </CollapsibleTrigger>
          <CollapsibleContent>
            <ul className="flex flex-col gap-1 text-xs">
              {offer.files.map((f) => (
                <li key={f.name} className="flex justify-between gap-2">
                  <span className="truncate font-mono">{f.name}</span>
                  <span className="text-muted-foreground tabular-nums">
                    {formatBytes(f.size)}
                  </span>
                </li>
              ))}
            </ul>
          </CollapsibleContent>
        </Collapsible>
      )}
    </div>
  );
}
