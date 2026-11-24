import { useState } from "react";
import { Button } from "@/components/ui/button";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import { FieldGroup } from "@/components/ui/field";
import { t } from "@/i18n";
import { copyText } from "@/lib/clipboard";
import { errorMessage } from "@/lib/format";
import { CheckIcon, CopyIcon } from "lucide-react";
import { toast } from "sonner";
import { SwitchField, TextField } from "./fields";
import { useLocalSocks, useSetLocalSocks } from "../../queries";
import type { ProxyNode } from "../../types";

/** The address LAN devices reach the router at: the one this page came from, unless the ports stay on loopback. */
function socksHost(listen?: string) {
  if (listen === "127.0.0.1" || listen === "::1") return "127.0.0.1";
  const h = window.location.hostname;
  return h.includes(":") && !h.startsWith("[") ? `[${h}]` : h;
}

/**
 * 本地 SOCKS5 (one-node's per-node local port): the node's own SOCKS5 port on the router, for the LAN. What a
 * device sends to it leaves through this node (with its 前置代理), whatever the proxy or the selection is doing.
 * Applied at once, apart from the form's save.
 */
export function LocalSocksCard({ node }: { node: ProxyNode }) {
  const port = node.local_socks?.port;
  const set = useSetLocalSocks();
  const status = useLocalSocks(!!port);
  const [draft, setDraft] = useState(port ? String(port) : "");
  const [shown, setShown] = useState(port);
  const [copied, setCopied] = useState(false);
  // the saved port changed (switched on, moved): the field follows it
  if (shown !== port) {
    setShown(port);
    setDraft(port ? String(port) : "");
  }

  const state = port
    ? status.data?.ports.find((p) => p.id === node.id)
    : undefined;
  const typed = Number(draft);
  const valid = Number.isInteger(typed) && typed >= 1024 && typed <= 65535;
  const changed = !!port && draft !== String(port);
  const url = port ? `socks5://${socksHost(status.data?.listen)}:${port}` : "";

  const apply = (enabled: boolean, next?: number) =>
    set.mutate(
      { id: node.id, enabled, port: next },
      {
        onSuccess: (r) => {
          const s = r.local_socks.ports.find((p) => p.id === node.id);
          if (enabled && s && !s.running)
            toast.error(t("proxy.socks.notStarted"), {
              description: s.error,
            });
        },
        onError: () => setDraft(port ? String(port) : ""),
      },
    );

  const copy = async () => {
    try {
      await copyText(url);
      setCopied(true);
      setTimeout(() => setCopied(false), 1500);
      toast.success(t("proxy.socks.copied"));
    } catch (e) {
      toast.error(t("proxy.node.copyFailed"), {
        description: errorMessage(e),
      });
    }
  };

  const hint = !port
    ? t("proxy.socks.hintOff")
    : state && !state.running
      ? t("proxy.socks.down", { error: state.error ?? "—" })
      : status.data?.error
        ? t("proxy.socks.down", { error: status.data.error })
        : t("proxy.socks.hintOn");

  return (
    <Card>
      <CardHeader>
        <CardTitle>{t("proxy.socks.title")}</CardTitle>
        <CardDescription>{t("proxy.socks.description")}</CardDescription>
      </CardHeader>
      <CardContent>
        <FieldGroup>
          <SwitchField
            label={t("proxy.socks.label")}
            hint={hint}
            checked={!!port}
            disabled={set.isPending || !!node.problem}
            onChange={(on) => apply(on)}
          />
          {port ? (
            <>
              <div className="grid items-end gap-2 sm:grid-cols-[8rem_auto_minmax(0,1fr)]">
                <TextField
                  label={t("proxy.socks.port")}
                  value={draft}
                  onChange={(v) => setDraft(v.replace(/\D/g, "").slice(0, 5))}
                  placeholder="1080"
                  inputMode="numeric"
                  invalid={changed && !valid}
                  mono
                />
                {changed && (
                  <Button
                    type="button"
                    variant="outline"
                    disabled={set.isPending || !valid}
                    onClick={() => apply(true, typed)}
                  >
                    {t("proxy.socks.usePort", { port: draft })}
                  </Button>
                )}
              </div>
              <div className="flex min-w-0 items-center gap-2 rounded-md bg-muted px-3 py-2">
                <code className="min-w-0 flex-1 truncate font-mono text-xs">
                  {url}
                </code>
                <Button
                  type="button"
                  variant="ghost"
                  size="sm"
                  onClick={() => void copy()}
                >
                  {copied ? (
                    <CheckIcon data-icon="inline-start" />
                  ) : (
                    <CopyIcon data-icon="inline-start" />
                  )}
                  {copied ? t("common.action.copied") : t("proxy.socks.copy")}
                </Button>
              </div>
            </>
          ) : null}
        </FieldGroup>
      </CardContent>
    </Card>
  );
}
