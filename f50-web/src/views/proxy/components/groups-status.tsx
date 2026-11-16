import { Button } from "@/components/ui/button";
import { SweepShine } from "@/components/ui/sweep-shine";
import { t } from "@/i18n";
import { cn } from "@/lib/utils";
import { useAtom } from "jotai";
import { ZapIcon } from "lucide-react";
import { NodeFlag } from "./node-flag";
import { NodeGroup } from "./node-group";
import { routingLabel } from "../lib/configs";
import { plainName } from "../lib/country";
import { latencyTone } from "../lib/format";
import { groupTypeLabel } from "../lib/route-doc";
import { useGroupSelect, useGroupTest } from "../queries";
import { foldedAtom } from "../store";
import type { GroupNode, GroupStatus, Routing } from "../types";

function delayText(n: GroupNode) {
  if (!n.alive) return t("proxy.node.timeout");
  return n.delay_ms === null ? null : `${n.delay_ms} ms`;
}

/** The 策略组 of the config in effect: who each one uses, the delays, and the pick for 手动选择. */
export function GroupsStatus({
  groups,
  routing,
}: {
  groups: GroupStatus[];
  routing: Routing;
}) {
  const [folded, setFolded] = useAtom(foldedAtom);
  if (!groups.length) return null;
  return (
    <section aria-label={t("proxy.groups.title")}>
      <div className="grid items-start gap-3 lg:grid-cols-2 2xl:grid-cols-3">
        {groups.map((g) => (
          <GroupBox
            key={g.name}
            group={g}
            note={
              routing !== "rule"
                ? t("proxy.groups.idle", { mode: routingLabel(routing) })
                : null
            }
            open={!folded[`group:${g.name}`]}
            onOpenChange={(open) =>
              setFolded((f) => ({ ...f, [`group:${g.name}`]: !open }))
            }
          />
        ))}
      </div>
    </section>
  );
}

function GroupBox({
  group,
  note,
  open,
  onOpenChange,
}: {
  group: GroupStatus;
  note: string | null;
  open: boolean;
  onOpenChange: (open: boolean) => void;
}) {
  const test = useGroupTest();
  const select = useGroupSelect();
  const pick = group.type === "select";
  const current = group.nodes.find((n) => n.name === group.selected);
  const delay = current && delayText(current);
  const description = [
    groupTypeLabel(group.type),
    group.selected &&
      t("proxy.groups.current", {
        name: group.selected + (delay ? ` · ${delay}` : ""),
      }),
    note,
  ]
    .filter(Boolean)
    .join(" · ");

  return (
    <NodeGroup
      title={group.name}
      description={description}
      open={open}
      onOpenChange={onOpenChange}
      actions={
        <Button
          variant="ghost"
          size="icon-sm"
          aria-label={t("proxy.groups.test", { name: group.name })}
          disabled={test.isPending}
          aria-busy={test.isPending}
          onClick={() => test.mutate(group.name)}
          className={cn(
            "text-muted-foreground hover:text-primary",
            test.isPending && "text-primary",
          )}
        >
          <ZapIcon className={cn(test.isPending && "animate-pulse")} />
        </Button>
      }
    >
      <div className="divide-y">
        {group.nodes.map((n) => {
          const selected = n.name === group.selected;
          const text = delayText(n);
          const choose = () =>
            pick &&
            !selected &&
            !select.isPending &&
            select.mutate({ group: group.name, node: n.name });
          return (
            <div
              key={n.name}
              role={pick ? "button" : undefined}
              tabIndex={pick ? 0 : undefined}
              aria-pressed={pick ? selected : undefined}
              aria-disabled={pick ? select.isPending : undefined}
              onClick={choose}
              onKeyDown={(e) => {
                if (pick && (e.key === "Enter" || e.key === " ")) {
                  e.preventDefault();
                  choose();
                }
              }}
              className={cn(
                "flex min-h-11 items-center gap-3 px-4 outline-none",
                pick && "focus-visible:bg-muted/60",
                selected
                  ? "bg-primary/[0.07]"
                  : pick && "cursor-pointer hover:bg-muted/50",
              )}
            >
              <NodeFlag name={n.name} />
              <span
                className={cn(
                  "min-w-0 flex-1 truncate text-sm",
                  selected && "font-medium text-primary",
                  !n.alive && "text-muted-foreground",
                )}
              >
                {plainName(n.name)}
              </span>
              {test.isPending ? (
                <SweepShine className="text-xs">
                  {t("proxy.node.testing")}
                </SweepShine>
              ) : (
                text && (
                  <span
                    className={cn(
                      "text-xs tabular-nums",
                      n.delay_ms === null || !n.alive
                        ? "text-destructive"
                        : latencyTone(n.delay_ms),
                    )}
                  >
                    {text}
                  </span>
                )
              )}
            </div>
          );
        })}
      </div>
    </NodeGroup>
  );
}
