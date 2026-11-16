import { memo } from "react";
import { Link } from "react-router-dom";
import { Button } from "@/components/ui/button";
import { SweepShine } from "@/components/ui/sweep-shine";
import { t } from "@/i18n";
import { cn } from "@/lib/utils";
import { CircleAlertIcon, InfoIcon } from "lucide-react";
import { NodeFlag } from "./node-flag";
import { plainName } from "../lib/country";
import { latencyTone, nodeTags, TONE } from "../lib/format";
import type { Latency } from "../store";
import type { ProxyNode } from "../types";

/**
 * Top line: flag, name, the delay (click to test again) and the info link; below it the protocol in one quiet line.
 * The info link shows on hover on a wide screen. The selected node is the tinted tile, its name in the accent colour, with a green dot while the proxy runs through it.
 */
export const NodeRow = memo(function NodeRow({
  node,
  selected,
  connected,
  latency,
  via,
  disabled,
  onSelect,
  onProbe,
}: {
  node: ProxyNode;
  selected: boolean;
  connected: boolean;
  latency?: Latency;
  /** The name of the node this one connects through (前置代理). */
  via?: string;
  disabled: boolean;
  onSelect: (id: string) => void;
  onProbe: (id: string) => void;
}) {
  const bad = Boolean(node.problem);
  const select = () => !bad && !disabled && !selected && onSelect(node.id);
  return (
    <div
      role="button"
      tabIndex={bad ? -1 : 0}
      aria-pressed={selected}
      aria-disabled={bad || disabled}
      onClick={select}
      onKeyDown={(e) => {
        if (e.key === "Enter" || e.key === " ") {
          e.preventDefault();
          select();
        }
      }}
      className={cn(
        "group/row flex flex-col gap-1.5 py-2.5 pr-2 pl-4 outline-none focus-visible:bg-muted/60 md:rounded-lg md:pl-3",
        selected
          ? "bg-primary/[0.07] md:bg-primary/10"
          : cn(
              "md:bg-muted/50",
              !bad &&
                !disabled &&
                "cursor-pointer hover:bg-muted/50 md:hover:bg-muted",
            ),
      )}
    >
      <div className="flex min-w-0 items-center gap-2">
        <NodeFlag name={node.name} className={cn(bad && "opacity-40")} />
        <span
          className={cn(
            "truncate text-sm",
            selected && "font-medium text-primary",
            bad && "text-muted-foreground",
          )}
        >
          {plainName(node.name)}
        </span>
        {connected && (
          <span
            role="img"
            aria-label={t("common.state.connected")}
            className={cn("size-1.5 shrink-0 rounded-full", TONE.goodBg)}
          />
        )}
        <div className="ml-auto flex shrink-0 items-center">
          {!bad && (
            <Button
              variant="ghost"
              size="xs"
              aria-label={t("proxy.node.probe", { name: node.name })}
              onClick={(e) => {
                e.stopPropagation();
                onProbe(node.id);
              }}
              className={cn(
                "h-6 px-1.5 text-xs tabular-nums",
                typeof latency === "number"
                  ? latencyTone(latency)
                  : latency === "fail"
                    ? "text-destructive"
                    : "text-muted-foreground",
              )}
            >
              {latency === "pending" ? (
                <SweepShine>{t("proxy.node.testing")}</SweepShine>
              ) : latency === "fail" ? (
                t("proxy.node.timeout")
              ) : typeof latency === "number" ? (
                `${latency} ms`
              ) : (
                <span className="opacity-0 group-hover/row:opacity-100 group-focus-within/row:opacity-100 max-md:opacity-100">
                  {t("proxy.node.test")}
                </span>
              )}
            </Button>
          )}
          <Button
            asChild
            variant="ghost"
            size="icon-xs"
            className="size-6 text-muted-foreground/70 group-focus-within/row:opacity-100 group-hover/row:opacity-100 hover:text-primary md:opacity-0"
          >
            <Link
              to={`/proxy/node?id=${encodeURIComponent(node.id)}`}
              aria-label={t("proxy.node.edit", { name: node.name })}
              title={t("proxy.node.edit", { name: node.name })}
              onClick={(e) => e.stopPropagation()}
            >
              <InfoIcon />
            </Link>
          </Button>
        </div>
      </div>
      {bad ? (
        <span
          title={node.problem ?? undefined}
          className="flex min-w-0 items-center gap-1 pl-8 text-xs text-destructive"
        >
          <CircleAlertIcon className="size-3 shrink-0" />
          <span className="truncate">{t("proxy.node.broken")}</span>
        </span>
      ) : (
        <span
          title={node.summary}
          className="truncate pl-8 text-xs text-muted-foreground"
        >
          {[...nodeTags(node), via && t("proxy.node.viaTag", { name: via })]
            .filter(Boolean)
            .join(" · ")}
        </span>
      )}
    </div>
  );
});
