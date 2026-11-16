import { Field, FieldLabel } from "@/components/ui/field";
import {
  Select,
  SelectContent,
  SelectGroup,
  SelectItem,
  SelectLabel,
  SelectSeparator,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { t } from "@/i18n";
import { allNodes, subscriptionTitle } from "../../lib/format";
import type { Protocol } from "../../lib/node-form";
import type { Profile, ProxyNode } from "../../types";

const NONE = "__none__";

/** Whether `node`'s own 前置代理 chain runs through node `id` (picking it would loop). */
function reaches(all: ProxyNode[], node: ProxyNode, id: string) {
  let at: ProxyNode | undefined = node;
  for (let hops = 0; at?.via && hops < 16; hops++) {
    if (at.via === id) return true;
    const next: string = at.via;
    at = all.find((n) => n.id === next);
  }
  return false;
}

/**
 * 前置代理 (one-node's `via-page.tsx` as a picker): the node this one connects through. Left out: the node itself,
 * broken nodes, nodes whose own chain comes back here, and for Hysteria2 (UDP) HTTP proxies, which carry no UDP.
 */
export function ViaField({
  profile,
  id,
  protocol,
  value,
  onChange,
}: {
  profile: Profile;
  /** The node being edited; undefined while adding one. */
  id?: string;
  protocol: Protocol;
  value: string;
  onChange: (via: string) => void;
}) {
  const all = allNodes(profile);
  const usable = (n: ProxyNode) =>
    n.id !== id &&
    !n.problem &&
    !(id && reaches(all, n, id)) &&
    !(protocol === "hysteria2" && n.kind === "http");
  const sections = [
    { key: "local", title: t("proxy.nodes.local"), nodes: profile.local_nodes },
    ...profile.subscriptions.map((s) => ({
      key: s.id,
      title: subscriptionTitle(s),
      nodes: s.nodes,
    })),
  ]
    .map((s) => ({ ...s, nodes: s.nodes.filter(usable) }))
    .filter((s) => s.nodes.length);
  const current = all.find((n) => n.id === value);

  return (
    <Field>
      <FieldLabel htmlFor="proxy-via">{t("proxy.node.via")}</FieldLabel>
      <Select
        value={value || NONE}
        onValueChange={(v) => onChange(v === NONE ? "" : v)}
      >
        <SelectTrigger id="proxy-via" className="w-full">
          <SelectValue>
            {current?.name ?? (value ? value : t("proxy.node.viaNone"))}
          </SelectValue>
        </SelectTrigger>
        <SelectContent position="popper">
          <SelectItem value={NONE}>{t("proxy.node.viaNone")}</SelectItem>
          {sections.map((s) => (
            <SelectGroup key={s.key}>
              <SelectSeparator />
              <SelectLabel>{s.title}</SelectLabel>
              {s.nodes.map((n) => (
                <SelectItem key={n.id} value={n.id}>
                  <span className="flex flex-col">
                    <span>{n.name}</span>
                    <span className="text-xs text-muted-foreground">
                      {n.summary}
                    </span>
                  </span>
                </SelectItem>
              ))}
            </SelectGroup>
          ))}
        </SelectContent>
      </Select>
    </Field>
  );
}
