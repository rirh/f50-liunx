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
 * Top line: flag, name, the delay (click to test again) and the info link; below it the protocol as small tags spanning the tile, with the info link at the right end.
 * The info link shows on hover on a wide screen. The selected node is the tinted tile, its name in the accent colour, with a green dot while the proxy runs through it.
 * On a wide screen each node is a white tile with continuous corners; colour changes ease on Apple's curve and a press sinks it slightly.
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
      data-tile
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
        "group/row flex flex-col gap-2 px-4 py-2.5 outline-none focus-visible:bg-muted/60 md:squircle md:rounded-xl md:px-3.5 md:py-3",
        "transition-[background-color,scale] duration-300 ease-(--ease-apple) motion-reduce:transition-none",
        selected
          ? "bg-primary/[0.07] md:bg-primary/[0.11]"
          : cn(
              "md:bg-card",
              !bad &&
                !disabled &&
                "cursor-pointer hover:bg-muted/50 md:hover:bg-(--tile-hover) md:active:scale-[0.98]",
            ),
      )}
    >
      <div className="flex min-w-0 items-center gap-2">
        <NodeFlag name={node.name} className={cn(bad && "opacity-40")} />
        <span
          className={cn(
            "truncate text-sm font-medium transition-colors duration-300",
            selected && "text-primary",
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
              "-mr-1.5 ml-auto h-6 shrink-0 px-1.5 text-xs font-medium tabular-nums",
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
              <span
                key={latency}
                className="animate-in duration-500 ease-(--ease-apple) fade-in-0 slide-in-from-bottom-0.5"
              >
                {latency} ms
              </span>
            ) : (
              <span className="opacity-0 transition-opacity duration-200 group-hover/row:opacity-100 group-focus-within/row:opacity-100 max-md:opacity-100">
                {t("proxy.node.test")}
              </span>
            )}
          </Button>
        )}
      </div>
      <div className="flex min-w-0 items-center gap-2">
        {bad ? (
          <span
            title={node.problem ?? undefined}
            className="flex min-w-0 flex-1 items-center gap-1 text-xs text-destructive"
          >
            <CircleAlertIcon className="size-3 shrink-0" />
            <span className="truncate">{t("proxy.node.broken")}</span>
          </span>
        ) : (
          <div
            title={node.summary}
            className="flex min-w-0 flex-1 gap-1 overflow-hidden"
          >
            {[...nodeTags(node), via && t("proxy.node.viaTag", { name: via })]
              .filter((tag): tag is string => Boolean(tag))
              .map((tag) => (
                <span
                  key={tag}
                  className={cn(
                    "shrink-0 rounded-[5px] px-1.5 text-[11px] leading-[18px] font-medium whitespace-nowrap transition-colors duration-300",
                    selected
                      ? "bg-primary/[0.14] text-primary"
                      : "bg-foreground/[0.06] text-foreground/70",
                  )}
                >
                  {tag}
                </span>
              ))}
          </div>
        )}
        <Button
          asChild
          variant="ghost"
          size="icon-xs"
          className="-mr-1 size-5 shrink-0 text-muted-foreground/70 transition-opacity duration-200 group-focus-within/row:opacity-100 group-hover/row:opacity-100 hover:text-primary md:opacity-0"
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
  );
});
