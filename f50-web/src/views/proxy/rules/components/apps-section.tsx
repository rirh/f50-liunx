import type { ReactNode } from "react";
import { Button } from "@/components/ui/button";
import {
  Card,
  CardAction,
  CardContent,
  CardDescription,
  CardFooter,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import { Switch } from "@/components/ui/switch";
import { t } from "@/i18n";
import { cn } from "@/lib/utils";
import { OutboundSelect } from "./outbound-select";
import { APP_PACKS, appHint, appName } from "../../lib/app-packs";
import type { RouteApp } from "../../lib/route-doc";

/** 应用分流: one switch per app, and where it goes while on; the last row is where everything else goes. */
export function AppsSection({
  apps,
  onChange,
  groups,
  nodes,
  final,
}: {
  apps: RouteApp[];
  onChange: (apps: RouteApp[]) => void;
  groups: string[];
  nodes: string[];
  final: ReactNode;
}) {
  const on = new Map(apps.map((a) => [a.app, a]));
  // apps saved by a newer catalog (or written by hand) stay as they are
  const unknown = apps.filter((a) => !APP_PACKS.some((p) => p.id === a.app));

  const toggle = (id: string, enabled: boolean) => {
    const pack = APP_PACKS.find((p) => p.id === id)!;
    onChange(
      enabled
        ? [
            ...apps,
            {
              app: id,
              outbound: pack.outbound ?? "proxy",
              match: structuredClone(pack.match),
            },
          ]
        : apps.filter((a) => a.app !== id),
    );
  };
  const route = (id: string, outbound: string) =>
    onChange(apps.map((a) => (a.app === id ? { ...a, outbound } : a)));
  const enableAll = () =>
    onChange([
      ...apps,
      ...APP_PACKS.filter((p) => !on.has(p.id)).map((p) => ({
        app: p.id,
        outbound: p.outbound ?? "proxy",
        match: structuredClone(p.match),
      })),
    ]);

  return (
    <Card>
      <CardHeader>
        <CardTitle>{t("proxy.apps.title")}</CardTitle>
        <CardDescription>
          {t("proxy.apps.count", { count: apps.length })}
        </CardDescription>
        <CardAction className="flex gap-1">
          <Button
            size="sm"
            variant="ghost"
            disabled={APP_PACKS.every((p) => on.has(p.id))}
            onClick={enableAll}
          >
            {t("proxy.apps.all")}
          </Button>
          <Button
            size="sm"
            variant="ghost"
            disabled={!apps.length}
            onClick={() => onChange([])}
          >
            {t("proxy.apps.none")}
          </Button>
        </CardAction>
      </CardHeader>
      <CardContent className="px-0">
        <div className="grid divide-y md:grid-cols-2 md:divide-y-0 md:[&>*]:border-b">
          {APP_PACKS.map((pack) => {
            const app = on.get(pack.id);
            const Icon = pack.icon;
            const name = appName(pack);
            return (
              <div
                key={pack.id}
                className="flex min-h-14 items-center gap-3 px-4 py-2"
              >
                {pack.logo ? (
                  <img
                    src={pack.logo}
                    alt=""
                    className={cn(
                      "size-5 shrink-0 object-contain",
                      !app && "opacity-60 grayscale",
                    )}
                  />
                ) : (
                  <Icon
                    className={cn(
                      "size-5 shrink-0",
                      app ? "text-primary" : "text-muted-foreground",
                    )}
                  />
                )}
                <span className="flex min-w-0 flex-1 flex-col gap-0.5">
                  <span className="truncate text-sm">{name}</span>
                  <span className="truncate text-xs text-muted-foreground">
                    {appHint(pack)}
                  </span>
                </span>
                {app && (
                  <OutboundSelect
                    label={t("proxy.apps.where", { name })}
                    value={app.outbound}
                    onChange={(o) => route(pack.id, o)}
                    groups={groups}
                    nodes={nodes}
                  />
                )}
                <Switch
                  aria-label={name}
                  checked={Boolean(app)}
                  onCheckedChange={(v) => toggle(pack.id, v)}
                />
              </div>
            );
          })}
          {unknown.map((a) => (
            <div
              key={a.app}
              className="flex min-h-14 items-center gap-3 px-4 py-2"
            >
              <span className="size-5 shrink-0" />
              <span className="flex min-w-0 flex-1 flex-col gap-0.5">
                <span className="truncate font-mono text-xs">{a.app}</span>
                <span className="truncate text-xs text-muted-foreground">
                  {t("proxy.apps.unknown")}
                </span>
              </span>
              <OutboundSelect
                label={t("proxy.apps.where", { name: a.app })}
                value={a.outbound}
                onChange={(o) => route(a.app, o)}
                groups={groups}
                nodes={nodes}
              />
              <Switch
                aria-label={a.app}
                checked
                onCheckedChange={() => onChange(apps.filter((x) => x !== a))}
              />
            </div>
          ))}
        </div>
        <div className="px-4 pt-3">{final}</div>
      </CardContent>
      <CardFooter className="border-t text-xs text-muted-foreground">
        {t("proxy.apps.footer")}
      </CardFooter>
    </Card>
  );
}
