import { useState } from "react";
import { Link, useNavigate } from "react-router-dom";
import { PendingButton } from "@/components/f50/pending-button";
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { t } from "@/i18n";
import { errorMessage, formatAgo } from "@/lib/format";
import { cn } from "@/lib/utils";
import {
  CheckIcon,
  CircleAlertIcon,
  LinkIcon,
  PencilIcon,
  PlusIcon,
} from "lucide-react";
import { toast } from "sonner";
import { fetchConfig } from "../../api";
import {
  builtinSummary,
  isBuiltin,
  nameFromUrl,
  newConfigText,
  nextConfigName,
  routingLabel,
} from "../../lib/configs";
import { TONE } from "../../lib/format";
import { useSaveConfig, useSetProxy } from "../../queries";
import type { Config, Profile } from "../../types";
import { checkSummary } from "../check";

const URL_RE = /^https?:\/\/\S+$/;

/** 分流配置: the built-in default (read-only, right for almost everyone), then the user's own; a click switches. */
export function ConfigList({ profile }: { profile: Profile }) {
  const navigate = useNavigate();
  const set = useSetProxy();
  const save = useSaveConfig();
  const [linking, setLinking] = useState(false);
  const [link, setLink] = useState("");
  const [fetching, setFetching] = useState(false);
  const builtin = profile.configs.find(isBuiltin);
  const own = profile.configs.filter((c) => !isBuiltin(c));
  const active = profile.active_config;
  const current = profile.configs.find((c) => c.id === active);
  const busy = set.isPending || save.isPending || fetching;
  const open = (id: string) =>
    navigate(`/proxy/rules?id=${encodeURIComponent(id)}`);

  const select = (c: Config) =>
    c.id !== active &&
    set.mutate(
      { active_config: c.id },
      {
        onSuccess: () =>
          toast.success(t("proxy.rules.switched", { name: c.name })),
      },
    );

  const create = () =>
    save.mutate(
      { name: nextConfigName(profile.configs), text: newConfigText() },
      { onSuccess: (c) => open(c.id) },
    );

  const addFromLink = async () => {
    const url = link.trim();
    if (!URL_RE.test(url)) return;
    setFetching(true);
    try {
      const { text } = await fetchConfig(url);
      save.mutate(
        { name: nameFromUrl(url, profile.configs), text, url },
        {
          onSuccess: (c) => {
            toast.success(t("proxy.rules.added", { name: c.name }));
            setLink("");
            setLinking(false);
            open(c.id);
          },
        },
      );
    } catch (e) {
      toast.error(t("proxy.rules.fetchFailed"), {
        description: errorMessage(e),
      });
    } finally {
      setFetching(false);
    }
  };

  return (
    <div className="grid items-start gap-6 lg:grid-cols-2">
      {profile.routing !== "rule" && (
        <Alert className="lg:col-span-2">
          <AlertTitle>
            {t("proxy.rules.idleTitle", {
              mode: routingLabel(profile.routing),
            })}
          </AlertTitle>
          <AlertDescription>{t("proxy.rules.idleBody")}</AlertDescription>
        </Alert>
      )}
      {builtin && (
        <Card>
          <CardHeader>
            <CardTitle>{t("proxy.rules.builtinTitle")}</CardTitle>
            <CardDescription>{t("proxy.rules.builtinHint")}</CardDescription>
          </CardHeader>
          <CardContent>
            <ConfigRow
              config={builtin}
              active={builtin.id === active}
              disabled={busy}
              onSelect={() => select(builtin)}
            >
              <ul className="mt-2 flex flex-col gap-1 text-sm text-muted-foreground">
                {builtinSummary().map((item) => (
                  <li key={item.what} className="flex items-baseline gap-2">
                    <span className="min-w-0 flex-1">{item.what}</span>
                    <span
                      className={cn(
                        "shrink-0 text-xs",
                        item.proxy ? "text-primary" : TONE.good,
                      )}
                    >
                      {item.proxy
                        ? t("proxy.outbound.proxy")
                        : t("proxy.outbound.direct")}
                    </span>
                  </li>
                ))}
              </ul>
            </ConfigRow>
          </CardContent>
        </Card>
      )}
      <Card>
        <CardHeader>
          <CardTitle>{t("proxy.rules.mine")}</CardTitle>
          <CardDescription>{t("proxy.rules.mineHint")}</CardDescription>
        </CardHeader>
        <CardContent className="flex flex-col gap-2">
          <div className="flex flex-wrap gap-2">
            <Button
              size="sm"
              variant="outline"
              onClick={() => setLinking((v) => !v)}
            >
              <LinkIcon data-icon="inline-start" />
              {linking ? t("common.action.cancel") : t("proxy.rules.fromLink")}
            </Button>
            <PendingButton
              size="sm"
              pending={save.isPending && !fetching}
              disabled={busy}
              onClick={create}
            >
              <PlusIcon data-icon="inline-start" />
              {t("proxy.rules.new")}
            </PendingButton>
          </div>
          {linking && (
            <form
              className="flex flex-wrap items-center gap-2"
              onSubmit={(e) => {
                e.preventDefault();
                void addFromLink();
              }}
            >
              <Input
                autoFocus
                aria-label={t("proxy.rules.link")}
                value={link}
                onChange={(e) => setLink(e.target.value)}
                placeholder={t("proxy.rules.linkPlaceholder")}
                inputMode="url"
                spellCheck={false}
                autoCapitalize="off"
                className="min-w-0 flex-1 font-mono text-xs"
              />
              <PendingButton
                size="sm"
                type="submit"
                pending={fetching}
                disabled={!URL_RE.test(link.trim()) || busy}
              >
                {t("common.action.download")}
              </PendingButton>
            </form>
          )}
          {!own.length && !linking && (
            <p className="text-sm text-muted-foreground">
              {t("proxy.rules.noneYet")}
            </p>
          )}
          {own.map((c) => (
            <ConfigRow
              key={c.id}
              config={c}
              active={c.id === active}
              disabled={busy}
              onSelect={() => select(c)}
              edit
            />
          ))}
        </CardContent>
      </Card>
      {current && !!current.info?.warnings?.length && (
        <Alert className="lg:col-span-2">
          <CircleAlertIcon />
          <AlertTitle>
            {t("proxy.rules.warningsOf", { name: current.name })}
          </AlertTitle>
          <AlertDescription>
            <ul className="list-disc pl-4">
              {current.info.warnings.map((w) => (
                <li key={w}>{w}</li>
              ))}
            </ul>
          </AlertDescription>
        </Alert>
      )}
    </div>
  );
}

/** One config: name, the check's summary and when it changed; ✓ while in use; a click uses it. */
function ConfigRow({
  config,
  active,
  disabled,
  onSelect,
  edit,
  children,
}: {
  config: Config;
  active: boolean;
  disabled: boolean;
  onSelect: () => void;
  edit?: boolean;
  children?: React.ReactNode;
}) {
  const select = () => !disabled && !active && onSelect();
  const warnings = config.info?.warnings?.length ?? 0;
  const meta = [
    checkSummary(config.text, config.info, config.builtin),
    config.updated_at && formatAgo(config.updated_at),
    warnings && t("proxy.rules.warnings", { count: warnings }),
    config.url && t("proxy.rules.linked"),
  ].filter(Boolean);
  return (
    <div
      role="button"
      tabIndex={0}
      aria-pressed={active}
      aria-disabled={disabled}
      onClick={select}
      onKeyDown={(e) => {
        if (e.key === "Enter" || e.key === " ") {
          e.preventDefault();
          select();
        }
      }}
      className={cn(
        "rounded-lg px-3 py-2.5 outline-none transition-colors focus-visible:ring-3 focus-visible:ring-ring/50",
        active
          ? "bg-primary/[0.08]"
          : cn("bg-muted/40", !disabled && "cursor-pointer hover:bg-muted/70"),
      )}
    >
      <div className="flex items-center gap-2">
        <span className="flex min-w-0 flex-1 flex-col gap-0.5">
          <span className="flex items-center gap-2">
            <span className="truncate text-sm font-medium">{config.name}</span>
            {active && (
              <Badge variant="secondary" className="gap-1">
                <CheckIcon />
                {t("proxy.rules.inUse")}
              </Badge>
            )}
          </span>
          <span className="truncate text-xs text-muted-foreground tabular-nums">
            {meta.join(" · ")}
          </span>
        </span>
        {edit && (
          <Button
            asChild
            variant="ghost"
            size="icon-sm"
            className="text-muted-foreground hover:text-primary"
          >
            <Link
              to={`/proxy/rules?id=${encodeURIComponent(config.id)}`}
              aria-label={t("proxy.rules.edit", { name: config.name })}
              onClick={(e) => e.stopPropagation()}
            >
              <PencilIcon />
            </Link>
          </Button>
        )}
      </div>
      {children}
    </div>
  );
}
