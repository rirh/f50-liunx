import { useId, useState } from "react";
import { Button } from "@/components/ui/button";
import {
  Card,
  CardAction,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import { Field, FieldError, FieldLabel } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { t } from "@/i18n";
import { cn } from "@/lib/utils";
import { ChevronDownIcon, PlusIcon, Trash2Icon } from "lucide-react";
import {
  DEFAULT_INTERVAL,
  filterRegex,
  GROUP_TYPES,
  groupMembers,
  groupNameProblem,
  groupTypeHint,
  groupTypeLabel,
  MIN_INTERVAL,
  type RouteGroup,
} from "../../lib/route-doc";

function nextGroupName(groups: RouteGroup[], nodes: string[]) {
  const taken = new Set([...groups.map((g) => g.name), ...nodes]);
  for (let i = 1; ; i++) {
    const name = t("proxy.rules.groups.nth", { n: i });
    if (!taken.has(name)) return name;
  }
}

/** 策略组: a name apps and rules can send traffic to, picking among nodes by hand, by speed, or the first that works. */
export function GroupsSection({
  groups,
  nodes,
  onChange,
  onRename,
  onRemove,
}: {
  groups: RouteGroup[];
  /** Node names, for members and name clashes. */
  nodes: string[];
  onChange: (groups: RouteGroup[]) => void;
  onRename: (index: number, from: string, to: string) => void;
  onRemove: (index: number) => void;
}) {
  const [open, setOpen] = useState<number | null>(null);
  const add = () => {
    onChange([
      ...groups,
      { name: nextGroupName(groups, nodes), type: "select", nodes: [] },
    ]);
    setOpen(groups.length);
  };
  const patch = (i: number, next: RouteGroup) =>
    onChange(groups.map((g, j) => (j === i ? next : g)));

  return (
    <Card>
      <CardHeader>
        <CardTitle>{t("proxy.groups.title")}</CardTitle>
        <CardDescription>{t("proxy.rules.groups.footer")}</CardDescription>
        <CardAction>
          <Button size="sm" variant="outline" onClick={add}>
            <PlusIcon data-icon="inline-start" />
            {t("common.action.add")}
          </Button>
        </CardAction>
      </CardHeader>
      <CardContent className="px-0">
        {groups.length === 0 && (
          <p className="px-4 text-sm text-muted-foreground">
            {t("proxy.rules.groups.empty")}
          </p>
        )}
        <div className="divide-y border-y empty:hidden">
          {groups.map((g, i) => {
            const members = groupMembers(g, nodes);
            const expanded = open === i;
            return (
              <div key={i}>
                <button
                  type="button"
                  aria-expanded={expanded}
                  onClick={() => setOpen(expanded ? null : i)}
                  className="flex min-h-11 w-full items-center gap-2.5 px-4 text-left text-sm outline-none hover:bg-muted/50 focus-visible:bg-muted/60"
                >
                  <ChevronDownIcon
                    className={cn(
                      "size-4 shrink-0 text-primary transition-transform duration-200 motion-reduce:transition-none",
                      !expanded && "-rotate-90",
                    )}
                  />
                  <span className="min-w-0 flex-1 truncate">{g.name}</span>
                  <span
                    className={cn(
                      "shrink-0 text-xs",
                      members.length
                        ? "text-muted-foreground"
                        : "text-destructive",
                    )}
                  >
                    {groupTypeLabel(g.type)} ·{" "}
                    {members.length
                      ? t("proxy.rules.groups.members", {
                          count: members.length,
                        })
                      : t("proxy.rules.groups.noMembers")}
                  </span>
                </button>
                {expanded && (
                  <GroupEditor
                    group={g}
                    others={groups.filter((_, j) => j !== i).map((x) => x.name)}
                    nodes={nodes}
                    onChange={(next) => patch(i, next)}
                    onRename={(to) => onRename(i, g.name, to)}
                    onRemove={() => {
                      setOpen(null);
                      onRemove(i);
                    }}
                  />
                )}
              </div>
            );
          })}
        </div>
      </CardContent>
    </Card>
  );
}

function GroupEditor({
  group,
  others,
  nodes,
  onChange,
  onRename,
  onRemove,
}: {
  group: RouteGroup;
  others: string[];
  nodes: string[];
  onChange: (group: RouteGroup) => void;
  onRename: (name: string) => void;
  onRemove: () => void;
}) {
  const picked = group.nodes ?? [];
  const re = group.filter ? filterRegex(group.filter) : null;
  const matched = re ? nodes.filter((n) => re.test(n)) : [];
  const timed = group.type !== "select";
  const filterId = useId();
  const typeId = useId();
  const togglePick = (name: string) =>
    onChange({
      ...group,
      nodes: picked.includes(name)
        ? picked.filter((n) => n !== name)
        : [...picked, name],
    });

  return (
    <div className="flex flex-col gap-4 bg-muted/30 px-4 py-4">
      <div className="grid gap-4 sm:grid-cols-2">
        <CommitField
          label={t("proxy.rules.groups.name")}
          value={group.name}
          problem={(v) => groupNameProblem(v, others, nodes)}
          onCommit={(v) => onRename(v.trim())}
        />
        <Field>
          <FieldLabel htmlFor={typeId}>
            {t("proxy.rules.groups.type")}
          </FieldLabel>
          <Select
            value={group.type}
            onValueChange={(v) => {
              const next: RouteGroup = {
                ...group,
                type: v as RouteGroup["type"],
              };
              if (v === "select") delete next.interval;
              onChange(next);
            }}
          >
            <SelectTrigger id={typeId} className="w-full">
              <SelectValue>{groupTypeLabel(group.type)}</SelectValue>
            </SelectTrigger>
            <SelectContent position="popper">
              {GROUP_TYPES.map((type) => (
                <SelectItem key={type} value={type}>
                  <span className="flex flex-col">
                    <span>{groupTypeLabel(type)}</span>
                    <span className="text-xs text-muted-foreground">
                      {groupTypeHint(type)}
                    </span>
                  </span>
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
        </Field>
      </div>
      <Field>
        <FieldLabel>{t("proxy.rules.groups.nodes")}</FieldLabel>
        <div className="flex flex-wrap gap-1.5">
          {[...nodes, ...picked.filter((n) => !nodes.includes(n))].map((n) => {
            const at = picked.indexOf(n);
            const gone = !nodes.includes(n);
            return (
              <button
                key={n}
                type="button"
                aria-pressed={at >= 0}
                onClick={() => togglePick(n)}
                className={cn(
                  "flex h-7 items-center gap-1 rounded-md px-2 text-xs outline-none focus-visible:ring-3 focus-visible:ring-ring/50",
                  at >= 0
                    ? "bg-primary text-primary-foreground"
                    : "bg-secondary text-secondary-foreground hover:bg-secondary/80",
                  gone && "bg-destructive/15 text-destructive line-through",
                )}
              >
                {at >= 0 && (
                  <span className="tabular-nums opacity-80">{at + 1}</span>
                )}
                {n}
              </button>
            );
          })}
          {!nodes.length && !picked.length && (
            <span className="text-xs text-muted-foreground">
              {t("proxy.rules.groups.noNodes")}
            </span>
          )}
        </div>
      </Field>
      <div className="grid gap-4 sm:grid-cols-2">
        <Field data-invalid={(Boolean(group.filter) && !re) || undefined}>
          <FieldLabel htmlFor={filterId}>
            {t("proxy.rules.groups.filter")}
          </FieldLabel>
          <Input
            id={filterId}
            value={group.filter ?? ""}
            placeholder={t("proxy.rules.groups.filterPlaceholder")}
            spellCheck={false}
            autoCapitalize="off"
            aria-invalid={(Boolean(group.filter) && !re) || undefined}
            className="font-mono text-xs"
            onChange={(e) => {
              const next: RouteGroup = { ...group, filter: e.target.value };
              if (!e.target.value) delete next.filter;
              onChange(next);
            }}
          />
          {group.filter && (
            <p
              className={cn(
                "text-xs",
                re ? "text-muted-foreground" : "text-destructive",
              )}
            >
              {!re
                ? t("proxy.rules.groups.filterBad")
                : matched.length
                  ? t("proxy.rules.groups.filterMatched", {
                      count: matched.length,
                      names: matched.join(t("proxy.listSep")),
                    })
                  : t("proxy.rules.groups.filterNone")}
            </p>
          )}
        </Field>
        {timed && (
          <CommitField
            label={t("proxy.rules.groups.interval")}
            value={String(group.interval ?? DEFAULT_INTERVAL)}
            inputMode="numeric"
            problem={(v) =>
              /^\d+$/.test(v.trim()) && Number(v) >= MIN_INTERVAL
                ? null
                : t("proxy.rules.groups.intervalMin", { n: MIN_INTERVAL })
            }
            onCommit={(v) => {
              const interval = Number(v.trim());
              const next: RouteGroup = { ...group, interval };
              if (interval === DEFAULT_INTERVAL) delete next.interval;
              onChange(next);
            }}
          />
        )}
      </div>
      <div>
        <Button size="sm" variant="destructive" onClick={onRemove}>
          <Trash2Icon data-icon="inline-start" />
          {t("proxy.rules.groups.delete")}
        </Button>
      </div>
    </div>
  );
}

/**
 * A field that applies on Enter or leaving it, and only when valid: a group's name is referenced elsewhere, so it
 * can't change one keystroke at a time.
 */
function CommitField({
  label,
  value,
  inputMode,
  problem,
  onCommit,
}: {
  label: string;
  value: string;
  inputMode?: React.HTMLAttributes<HTMLInputElement>["inputMode"];
  problem: (value: string) => string | null;
  onCommit: (value: string) => void;
}) {
  const [draft, setDraft] = useState<string | null>(null);
  const id = useId();
  const text = draft ?? value;
  const error = draft !== null ? problem(draft) : null;
  const commit = () => {
    if (draft !== null && !problem(draft) && draft.trim() !== value)
      onCommit(draft);
    setDraft(null);
  };
  return (
    <Field data-invalid={Boolean(error) || undefined}>
      <FieldLabel htmlFor={id}>{label}</FieldLabel>
      <Input
        id={id}
        value={text}
        inputMode={inputMode}
        spellCheck={false}
        autoCapitalize="off"
        aria-invalid={Boolean(error) || undefined}
        onChange={(e) => setDraft(e.target.value)}
        onBlur={commit}
        onKeyDown={(e) => {
          if (e.key === "Enter") {
            e.preventDefault();
            commit();
          }
          if (e.key === "Escape") setDraft(null);
        }}
      />
      {error && (
        <FieldError>{t("proxy.rules.groups.revert", { error })}</FieldError>
      )}
    </Field>
  );
}
