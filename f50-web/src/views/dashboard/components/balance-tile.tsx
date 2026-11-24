import { useState } from "react";
import { ConfirmButton } from "@/components/f50/confirm-button";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import {
  Popover,
  PopoverContent,
  PopoverTrigger,
} from "@/components/ui/popover";
import { t } from "@/i18n";
import { errorMessage, formatAgo, formatTime } from "@/lib/format";
import { cn } from "@/lib/utils";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { RefreshCwIcon } from "lucide-react";
import { toast } from "sonner";
import { STATUS_KEY, queryBalance, setBalance } from "../api";
import type { BalanceInfo } from "../types";

/**
 * 话费余额 beside the data used: the amount the carrier's last reply stated. The popover asks again (an SMS to the
 * carrier, answered within a minute or two) only when clicked and confirmed (the SMS may be charged; nothing asks
 * on a schedule), says when the amount was read, and holds the query (the SIM's carrier by default).
 */
export function BalanceTile({ b }: { b: BalanceInfo }) {
  const qc = useQueryClient();
  const [number, setNumber] = useState(b.settings.number);
  const [code, setCode] = useState(b.settings.code);
  const refresh = () => qc.invalidateQueries({ queryKey: STATUS_KEY });
  const ask = useMutation({
    mutationFn: queryBalance,
    onSuccess: () => {
      toast.success(t("dashboard.balance.asked"));
      void refresh();
    },
    onError: (e) =>
      toast.error(t("dashboard.balance.askFailed"), {
        description: errorMessage(e),
      }),
  });
  const save = useMutation({
    mutationFn: setBalance,
    onSuccess: () => {
      toast.success(t("dashboard.balance.saved"));
      void refresh();
    },
    onError: (e) =>
      toast.error(t("dashboard.balance.saveFailed"), {
        description: errorMessage(e),
      }),
  });

  const low = b.amount != null && b.amount < 10;
  const busy = b.pending || ask.isPending;
  const [yuan, cents] =
    b.amount != null ? b.amount.toFixed(2).split(".") : ["—", ""];

  return (
    <Popover>
      <PopoverTrigger asChild>
        <button
          type="button"
          className="flex shrink-0 flex-col items-start rounded-xl bg-muted/80 px-3 py-2 text-start transition-colors hover:bg-muted"
          aria-label={t("dashboard.balance.title")}
        >
          <span className="flex w-full items-baseline justify-between gap-3">
            <span className="text-xs font-medium text-muted-foreground">
              {t("dashboard.balance.title")}
            </span>
            {busy && (
              <RefreshCwIcon
                aria-hidden
                className="size-3 animate-spin self-center text-muted-foreground"
              />
            )}
          </span>
          <span
            className={cn(
              "tabular-nums text-base leading-tight font-semibold",
              low && "text-[#C93400] dark:text-[#FF9F0A]",
            )}
          >
            {b.amount != null && (
              <span className="me-0.5 text-xs font-medium">¥</span>
            )}
            {yuan}
            {cents && (
              <span className="text-xs font-medium text-muted-foreground">
                .{cents}
              </span>
            )}
          </span>
        </button>
      </PopoverTrigger>
      <PopoverContent align="end" className="flex w-72 flex-col gap-3">
        <div className="flex flex-col gap-0.5">
          <span className="text-sm font-medium">
            {t("dashboard.balance.title")}
            {b.amount != null && (
              <span className="ms-2 tabular-nums">¥{b.amount.toFixed(2)}</span>
            )}
          </span>
          <span
            className="text-xs text-muted-foreground"
            title={b.at ? formatTime(b.at) : undefined}
          >
            {b.pending
              ? t("dashboard.balance.pending")
              : b.at
                ? t("dashboard.balance.readAt", { ago: formatAgo(b.at) })
                : t("dashboard.balance.never")}
          </span>
          {b.error && (
            <span className="text-xs text-destructive">{b.error}</span>
          )}
        </div>
        <ConfirmButton
          size="sm"
          destructive={false}
          disabled={busy}
          title={t("dashboard.balance.confirmTitle")}
          description={t("dashboard.balance.confirmBody")}
          confirmText={t("dashboard.balance.confirmSend")}
          onConfirm={() => ask.mutate()}
        >
          <RefreshCwIcon
            data-icon="inline-start"
            className={cn(busy && "animate-spin")}
          />
          {busy ? t("dashboard.balance.asking") : t("dashboard.balance.ask")}
        </ConfirmButton>
        <div className="flex flex-col gap-2 border-t pt-3">
          <Label className="text-xs">{t("dashboard.balance.query")}</Label>
          <div className="grid grid-cols-2 gap-2">
            <Input
              value={number}
              onChange={(e) => setNumber(e.target.value.replace(/\D/g, ""))}
              placeholder={t("dashboard.balance.numberAuto")}
              inputMode="numeric"
              className="h-8 font-mono text-xs"
              aria-label={t("dashboard.balance.number")}
            />
            <Input
              value={code}
              onChange={(e) => setCode(e.target.value)}
              placeholder={t("dashboard.balance.codeAuto")}
              className="h-8 font-mono text-xs"
              aria-label={t("dashboard.balance.code")}
            />
          </div>
          <span className="text-xs text-muted-foreground">
            {t("dashboard.balance.hint")}
          </span>
          {(number !== b.settings.number || code !== b.settings.code) && (
            <Button
              size="sm"
              variant="outline"
              disabled={save.isPending}
              onClick={() => save.mutate({ number, code: code.trim() })}
            >
              {t("common.action.save")}
            </Button>
          )}
        </div>
      </PopoverContent>
    </Popover>
  );
}
