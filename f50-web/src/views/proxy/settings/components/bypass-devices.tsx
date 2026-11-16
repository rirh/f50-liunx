import { useMemo, useState } from "react";
import { PendingButton } from "@/components/f50/pending-button";
import { Button } from "@/components/ui/button";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { t } from "@/i18n";
import { getClients, systemKeys } from "@/views/system/overview/api";
import { useQuery } from "@tanstack/react-query";
import { PlusIcon, XIcon } from "lucide-react";
import { useSetSettings } from "../../queries";

const MAC_RE = /^([0-9a-f]{2}:){5}[0-9a-f]{2}$/;
const normal = (mac: string) => mac.trim().toLowerCase().replace(/-/g, ":");

interface Client {
  mac: string;
  name: string | null;
  ip: string | null;
}

/** The LAN clients the console already lists (Wi-Fi stations and DHCP leases), by MAC, with a name and address. */
function useLanClients(): Client[] {
  const { data } = useQuery({
    queryKey: systemKeys.clients,
    queryFn: getClients,
  });
  return useMemo(() => {
    const out = new Map<string, Client>();
    for (const l of data?.leases ?? [])
      out.set(normal(l.mac), { mac: normal(l.mac), name: l.host, ip: l.ip });
    for (const s of data?.wifi ?? []) {
      const mac = normal(s.mac);
      const known = out.get(mac);
      out.set(mac, {
        mac,
        name: known?.name ?? s.host,
        ip: known?.ip ?? s.ip,
      });
    }
    return [...out.values()];
  }, [data]);
}

/** 不走代理的设备: their traffic never reaches the proxy. Picked from the LAN clients, or a MAC typed in. */
export function BypassDevices({ macs }: { macs: string[] }) {
  const clients = useLanClients();
  const set = useSetSettings();
  const [pick, setPick] = useState("");
  const [typed, setTyped] = useState("");
  const typedMac = normal(typed);
  const typedOk = MAC_RE.test(typedMac) && !macs.includes(typedMac);
  const known = new Map(clients.map((c) => [c.mac, c]));
  const choices = clients.filter((c) => !macs.includes(c.mac));

  const save = (next: string[], done?: () => void) =>
    set.mutate({ bypass_macs: next.join(",") }, { onSuccess: done });

  return (
    <Card>
      <CardHeader>
        <CardTitle>{t("proxy.settings.bypass.title")}</CardTitle>
        <CardDescription>
          {t("proxy.settings.bypass.description")}
        </CardDescription>
      </CardHeader>
      <CardContent className="flex flex-col gap-4">
        {macs.length ? (
          <ul className="divide-y divide-border overflow-hidden rounded-xl bg-muted/70">
            {macs.map((mac) => {
              const c = known.get(mac);
              return (
                <li
                  key={mac}
                  className="flex min-h-11 items-center gap-3 px-3 py-1.5"
                >
                  <span className="flex min-w-0 flex-1 flex-col">
                    <span className="truncate text-sm">
                      {c?.name || t("proxy.settings.bypass.unknown")}
                    </span>
                    <span className="truncate text-xs text-muted-foreground tabular-nums">
                      {[mac, c?.ip].filter(Boolean).join(" · ")}
                    </span>
                  </span>
                  <Button
                    variant="ghost"
                    size="icon-sm"
                    aria-label={t("proxy.settings.bypass.remove", { mac })}
                    disabled={set.isPending}
                    onClick={() => save(macs.filter((m) => m !== mac))}
                    className="text-muted-foreground hover:text-destructive"
                  >
                    <XIcon />
                  </Button>
                </li>
              );
            })}
          </ul>
        ) : (
          <p className="text-sm text-muted-foreground">
            {t("proxy.settings.bypass.none")}
          </p>
        )}
        <div className="grid gap-2 sm:grid-cols-[minmax(0,1fr)_auto]">
          <Select
            value={pick}
            onValueChange={setPick}
            disabled={!choices.length}
          >
            <SelectTrigger
              className="w-full"
              aria-label={t("proxy.settings.bypass.pick")}
            >
              <SelectValue
                placeholder={
                  choices.length
                    ? t("proxy.settings.bypass.pick")
                    : t("proxy.settings.bypass.noClients")
                }
              />
            </SelectTrigger>
            <SelectContent position="popper">
              {choices.map((c) => (
                <SelectItem key={c.mac} value={c.mac}>
                  <span className="flex flex-col">
                    <span>{c.name || c.mac}</span>
                    <span className="text-xs text-muted-foreground tabular-nums">
                      {[c.mac, c.ip].filter(Boolean).join(" · ")}
                    </span>
                  </span>
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
          <PendingButton
            variant="outline"
            pending={set.isPending && !!pick}
            disabled={!pick}
            onClick={() => save([...macs, pick], () => setPick(""))}
          >
            <PlusIcon data-icon="inline-start" />
            {t("common.action.add")}
          </PendingButton>
          <Input
            value={typed}
            onChange={(e) => setTyped(e.target.value)}
            placeholder="aa:bb:cc:dd:ee:ff"
            aria-label={t("proxy.settings.bypass.mac")}
            aria-invalid={
              (!!typed.trim() && !MAC_RE.test(typedMac)) || undefined
            }
            spellCheck={false}
            autoCapitalize="off"
            className="font-mono text-xs"
          />
          <PendingButton
            variant="outline"
            pending={set.isPending && !!typed}
            disabled={!typedOk}
            onClick={() => save([...macs, typedMac], () => setTyped(""))}
          >
            <PlusIcon data-icon="inline-start" />
            {t("proxy.settings.bypass.addMac")}
          </PendingButton>
        </div>
      </CardContent>
    </Card>
  );
}
