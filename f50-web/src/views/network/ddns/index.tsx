import { useState } from "react";
import { PageHeader } from "@/components/f50/page-header";
import { PendingButton } from "@/components/f50/pending-button";
import { QueryState } from "@/components/f50/query-state";
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert";
import { Button } from "@/components/ui/button";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import {
  Field,
  FieldContent,
  FieldDescription,
  FieldGroup,
  FieldLabel,
} from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { Switch } from "@/components/ui/switch";
import { t } from "@/i18n";
import { errorMessage, formatAgo, formatTime } from "@/lib/format";
import { f50 } from "@/lib/ubus";
import { cn } from "@/lib/utils";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import {
  CircleAlertIcon,
  PlusIcon,
  RefreshCwIcon,
  Trash2Icon,
} from "lucide-react";
import { toast } from "sonner";

/** One record: `target` is `router` or a LAN device's MAC (AAAA only). */
interface DdnsRecord {
  name: string;
  type: "AAAA" | "A";
  target: string;
}

interface DdnsStatus {
  name: string;
  type: string;
  ip: string | null;
  updated_at: number | null;
  checked_at: number | null;
  error: string | null;
}

/** `f50.ddns get` (f50-assist/src/ddns.rs); the token is never returned. */
interface DdnsInfo {
  enabled: boolean;
  provider: "cloudflare";
  token_set: boolean;
  token_hint: string | null;
  zone: string;
  records: DdnsRecord[];
  interval_minutes: number;
  status: DdnsStatus[];
}

/** `f50.ddns neighbors`: a LAN device and the public IPv6 addresses the router has seen for it. */
interface Neighbor {
  host: string;
  ip: string;
  mac: string;
  ipv6: string[];
}

const KEY = ["f50.ddns"] as const;
const getDdns = () => f50<DdnsInfo>("f50.ddns", "get");
const setDdns = (c: Record<string, unknown>) =>
  f50<DdnsInfo>("f50.ddns", "set", c);
const updateDdns = () => f50<DdnsInfo>("f50.ddns", "update");
const getNeighbors = () =>
  f50<{ devices: Neighbor[] }>("f50.ddns", "neighbors").then((r) => r.devices);

const ROUTER = "router";
/** The target select's other choice: a LAN device by its IPv6 suffix. */
const DEVICE = "device";

/**
 * DDNS on Cloudflare: names kept at the router's cellular address (IPv6; IPv4 only when it is public) or at a LAN
 * device's IPv6 suffix on the LAN's current prefix (picked from the addresses the router has seen). The token is write-only. Saving applies at once and updates the records.
 */
export default function DdnsPage() {
  const query = useQuery({
    queryKey: KEY,
    queryFn: getDdns,
    refetchInterval: 15_000,
  });
  const qc = useQueryClient();
  const update = useMutation({
    mutationFn: updateDdns,
    onSuccess: (v) => {
      qc.setQueryData(KEY, v);
      const bad = v.status.filter((s) => s.error);
      if (bad.length)
        toast.error(t("network.ddns.updateSome"), {
          description: bad[0].error,
        });
      else toast.success(t("network.ddns.updated"));
    },
    onError: (e) =>
      toast.error(t("network.ddns.updateFailed"), {
        description: errorMessage(e),
      }),
  });
  return (
    <div className="flex flex-col gap-4">
      <PageHeader
        title={t("network.ddns.title")}
        description={t("network.ddns.description")}
        actions={
          query.data?.enabled && (
            <PendingButton
              variant="outline"
              size="sm"
              pending={update.isPending}
              onClick={() => update.mutate()}
            >
              <RefreshCwIcon data-icon="inline-start" />
              {t("network.ddns.updateNow")}
            </PendingButton>
          )
        }
      />
      <QueryState query={query} skeletonRows={6}>
        {(d) => (
          <div className="flex max-w-3xl flex-col gap-4">
            <StatusCard d={d} />
            <SettingsForm
              key={`${d.zone}|${d.token_hint}|${d.enabled}|${JSON.stringify(d.records)}|${d.interval_minutes}`}
              d={d}
            />
          </div>
        )}
      </QueryState>
    </div>
  );
}

function StatusCard({ d }: { d: DdnsInfo }) {
  if (!d.enabled || !d.records.length) return null;
  return (
    <Card>
      <CardHeader>
        <CardTitle>{t("network.ddns.status")}</CardTitle>
      </CardHeader>
      <CardContent>
        <ul className="flex flex-col divide-y">
          {d.records.map((r) => {
            const s = d.status.find(
              (x) => x.name === r.name && x.type === r.type,
            );
            return (
              <li
                key={`${r.name}-${r.type}`}
                className="flex flex-col gap-0.5 py-2 first:pt-0 last:pb-0"
              >
                <span className="flex items-center justify-between gap-2">
                  <span className="truncate font-mono text-sm">
                    {r.name}{" "}
                    <span className="text-xs text-muted-foreground">
                      {r.type}
                    </span>
                  </span>
                  <span
                    className={cn(
                      "inline-flex shrink-0 items-center gap-1.5 text-xs",
                      s?.error
                        ? "text-destructive"
                        : s?.ip
                          ? "text-emerald-600 dark:text-emerald-400"
                          : "text-muted-foreground",
                    )}
                  >
                    <span
                      className={cn(
                        "size-1.5 rounded-full",
                        s?.error
                          ? "bg-destructive"
                          : s?.ip
                            ? "bg-emerald-500"
                            : "bg-muted-foreground/50",
                      )}
                    />
                    {s?.error
                      ? t("network.ddns.failed")
                      : s?.ip
                        ? t("network.ddns.ok")
                        : t("network.ddns.waiting")}
                  </span>
                </span>
                {s?.ip && (
                  <span className="truncate font-mono text-xs text-muted-foreground">
                    {s.ip}
                  </span>
                )}
                {s?.error && (
                  <span className="text-xs text-destructive">{s.error}</span>
                )}
                {(s?.updated_at || s?.checked_at) && (
                  <span
                    className="text-xs text-muted-foreground"
                    title={formatTime(s.updated_at ?? s.checked_at)}
                  >
                    {s.updated_at
                      ? t("network.ddns.updatedAt", {
                          ago: formatAgo(s.updated_at),
                        })
                      : ""}
                    {s.checked_at
                      ? ` · ${t("network.ddns.checkedAt", { ago: formatAgo(s.checked_at) })}`
                      : ""}
                  </span>
                )}
              </li>
            );
          })}
        </ul>
      </CardContent>
    </Card>
  );
}

function SettingsForm({ d }: { d: DdnsInfo }) {
  const qc = useQueryClient();
  const neighbors = useQuery({
    queryKey: ["f50.ddns", "neighbors"],
    queryFn: getNeighbors,
  });
  const [enabled, setEnabled] = useState(d.enabled);
  const [token, setToken] = useState("");
  const [zone, setZone] = useState(d.zone);
  const [every, setEvery] = useState(String(d.interval_minutes));
  const [records, setRecords] = useState<DdnsRecord[]>(
    d.records.length ? d.records : [{ name: "", type: "AAAA", target: ROUTER }],
  );
  const [error, setError] = useState<string | null>(null);
  const save = useMutation({
    mutationFn: setDdns,
    onSuccess: (v) => {
      qc.setQueryData(KEY, v);
      toast.success(t("network.ddns.saved"));
    },
    onError: (e) => setError(errorMessage(e)),
  });

  const filled = records.filter((r) => r.name.trim());
  const dirty =
    enabled !== d.enabled ||
    !!token ||
    zone !== d.zone ||
    every !== String(d.interval_minutes) ||
    JSON.stringify(filled) !== JSON.stringify(d.records);
  const patch = (i: number, p: Partial<DdnsRecord>) =>
    setRecords((rs) => rs.map((r, j) => (j === i ? { ...r, ...p } : r)));
  const seen = (neighbors.data ?? []).filter((n) => n.ipv6.length);

  return (
    <Card>
      <CardHeader>
        <CardTitle>{t("network.ddns.settings")}</CardTitle>
        <CardDescription>{t("network.ddns.cloudflareHint")}</CardDescription>
      </CardHeader>
      <CardContent>
        <form
          className="flex flex-col gap-5"
          onSubmit={(e) => {
            e.preventDefault();
            setError(null);
            save.mutate({
              enabled,
              zone: zone.trim(),
              interval_minutes: Number(every) || 5,
              records: JSON.stringify(filled),
              ...(token.trim() ? { token: token.trim() } : {}),
            });
          }}
        >
          <FieldGroup>
            <Field orientation="horizontal">
              <FieldContent>
                <FieldLabel htmlFor="ddns-enabled">
                  {t("network.ddns.enabled")}
                </FieldLabel>
                <FieldDescription>
                  {t("network.ddns.enabledHint")}
                </FieldDescription>
              </FieldContent>
              <Switch
                id="ddns-enabled"
                checked={enabled}
                onCheckedChange={setEnabled}
              />
            </Field>
            <div className="grid gap-4 sm:grid-cols-2">
              <Field>
                <FieldLabel htmlFor="ddns-token">
                  {t("network.ddns.token")}
                </FieldLabel>
                <Input
                  id="ddns-token"
                  type="password"
                  value={token}
                  onChange={(e) => setToken(e.target.value)}
                  placeholder={
                    d.token_set
                      ? t("network.ddns.tokenKeep", {
                          hint: d.token_hint ?? "",
                        })
                      : "Cloudflare API Token"
                  }
                  autoComplete="off"
                  spellCheck={false}
                  className="font-mono text-base sm:text-sm"
                />
                <FieldDescription>
                  {t("network.ddns.tokenHint")}
                </FieldDescription>
              </Field>
              <Field>
                <FieldLabel htmlFor="ddns-zone">
                  {t("network.ddns.zone")}
                </FieldLabel>
                <Input
                  id="ddns-zone"
                  value={zone}
                  onChange={(e) => setZone(e.target.value.trim())}
                  placeholder="example.com"
                  autoComplete="off"
                  spellCheck={false}
                  className="font-mono text-base sm:text-sm"
                />
                <FieldDescription>
                  {t("network.ddns.zoneHint")}
                </FieldDescription>
              </Field>
            </div>
          </FieldGroup>

          <div className="flex flex-col gap-2">
            <span className="text-sm font-medium">
              {t("network.ddns.records")}
            </span>
            {records.map((r, i) => (
              <div
                key={i}
                className="grid gap-2 rounded-lg bg-muted/50 p-2 sm:grid-cols-[minmax(0,1fr)_5.5rem_minmax(0,1fr)_auto]"
              >
                <Input
                  value={r.name}
                  onChange={(e) =>
                    patch(i, { name: e.target.value.trim().toLowerCase() })
                  }
                  placeholder={zone ? `home.${zone}` : "home.example.com"}
                  aria-label={t("network.ddns.name")}
                  className="bg-background font-mono text-base sm:text-sm"
                />
                <Select
                  value={r.type}
                  onValueChange={(v) =>
                    patch(i, {
                      type: v as DdnsRecord["type"],
                      ...(v === "A" ? { target: ROUTER } : {}),
                    })
                  }
                >
                  <SelectTrigger
                    className="w-full bg-background"
                    aria-label={t("network.ddns.type")}
                  >
                    <SelectValue />
                  </SelectTrigger>
                  <SelectContent position="popper">
                    <SelectItem value="AAAA">AAAA</SelectItem>
                    <SelectItem value="A">A</SelectItem>
                  </SelectContent>
                </Select>
                <Select
                  value={r.target === ROUTER ? ROUTER : DEVICE}
                  onValueChange={(v) =>
                    patch(i, { target: v === ROUTER ? ROUTER : "" })
                  }
                  disabled={r.type === "A"}
                >
                  <SelectTrigger
                    className="w-full bg-background"
                    aria-label={t("network.ddns.target")}
                  >
                    <SelectValue />
                  </SelectTrigger>
                  <SelectContent position="popper">
                    <SelectItem value={ROUTER}>
                      {t("network.ddns.router")}
                    </SelectItem>
                    <SelectItem value={DEVICE}>
                      {t("network.ddns.device")}
                    </SelectItem>
                  </SelectContent>
                </Select>
                <Button
                  type="button"
                  variant="ghost"
                  size="icon"
                  aria-label={t("network.ddns.remove")}
                  onClick={() =>
                    setRecords((rs) => rs.filter((_, j) => j !== i))
                  }
                >
                  <Trash2Icon />
                </Button>
                {r.target !== ROUTER && r.type === "AAAA" && (
                  <div className="grid gap-2 sm:col-span-full sm:grid-cols-[minmax(0,1fr)_minmax(0,1fr)]">
                    <Input
                      value={r.target}
                      onChange={(e) =>
                        patch(i, {
                          target: e.target.value.trim().toLowerCase(),
                        })
                      }
                      placeholder="::1cf3:85ff:c719:c51c"
                      aria-label={t("network.ddns.suffix")}
                      className="bg-background font-mono text-base sm:text-sm"
                    />
                    <Select
                      value=""
                      onValueChange={(v) => patch(i, { target: v })}
                    >
                      <SelectTrigger
                        className="w-full bg-background"
                        aria-label={t("network.ddns.pick")}
                      >
                        <SelectValue placeholder={t("network.ddns.pick")} />
                      </SelectTrigger>
                      <SelectContent position="popper">
                        {seen.length === 0 && (
                          <div className="px-2 py-1.5 text-xs text-muted-foreground">
                            {t("network.ddns.noneSeen")}
                          </div>
                        )}
                        {seen.map((dev) =>
                          dev.ipv6.map((a) => (
                            <SelectItem key={a} value={a}>
                              <span className="flex flex-col">
                                <span>{dev.host || dev.mac}</span>
                                <span className="font-mono text-xs text-muted-foreground">
                                  {a}
                                </span>
                              </span>
                            </SelectItem>
                          )),
                        )}
                      </SelectContent>
                    </Select>
                  </div>
                )}
              </div>
            ))}
            <Button
              type="button"
              variant="outline"
              size="sm"
              className="self-start"
              disabled={records.length >= 16}
              onClick={() =>
                setRecords((rs) => [
                  ...rs,
                  { name: "", type: "AAAA", target: ROUTER },
                ])
              }
            >
              <PlusIcon data-icon="inline-start" />
              {t("network.ddns.add")}
            </Button>
            <p className="text-xs text-muted-foreground">
              {t("network.ddns.recordsHint")}
            </p>
          </div>

          <Field className="max-w-48">
            <FieldLabel htmlFor="ddns-interval">
              {t("network.ddns.interval")}
            </FieldLabel>
            <Input
              id="ddns-interval"
              value={every}
              onChange={(e) => setEvery(e.target.value.replace(/\D/g, ""))}
              inputMode="numeric"
              className="text-base sm:text-sm"
            />
          </Field>

          <Alert>
            <CircleAlertIcon />
            <AlertTitle>{t("network.ddns.firewallTitle")}</AlertTitle>
            <AlertDescription>{t("network.ddns.firewall")}</AlertDescription>
          </Alert>
          {error && (
            <Alert variant="destructive">
              <CircleAlertIcon />
              <AlertDescription>{error}</AlertDescription>
            </Alert>
          )}
          <div className="flex justify-end">
            <PendingButton
              type="submit"
              size="sm"
              pending={save.isPending}
              disabled={!dirty}
            >
              {t("common.action.save")}
            </PendingButton>
          </div>
        </form>
      </CardContent>
    </Card>
  );
}
