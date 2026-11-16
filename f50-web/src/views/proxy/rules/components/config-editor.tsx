import { useState } from "react";
import { useNavigate } from "react-router-dom";
import { ConfirmButton } from "@/components/f50/confirm-button";
import { PageHeader } from "@/components/f50/page-header";
import { PendingButton } from "@/components/f50/pending-button";
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import {
  Card,
  CardAction,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import { Field, FieldGroup, FieldLabel } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { Textarea } from "@/components/ui/textarea";
import { t } from "@/i18n";
import { errorMessage, formatAgo } from "@/lib/format";
import {
  ArrowLeftIcon,
  CheckIcon,
  CircleAlertIcon,
  ListChecksIcon,
  RefreshCwIcon,
  SlidersHorizontalIcon,
} from "lucide-react";
import { toast } from "sonner";
import { AppsSection } from "./apps-section";
import { CustomRules } from "./custom-rules";
import { GroupsSection } from "./groups-section";
import { OutboundSelect } from "./outbound-select";
import { checkRules, fetchConfig } from "../../api";
import { allNodes } from "../../lib/format";
import {
  compactJson,
  docForSave,
  formatDoc,
  parseDoc,
  renameOutbound,
  type RouteDoc,
  sameDoc,
} from "../../lib/route-doc";
import { useDeleteConfig, useSaveConfig, useSetProxy } from "../../queries";
import type { Config, Profile, RulesInfo } from "../../types";
import { checkSummary } from "../check";
import { useRulesText } from "../use-rules-text";

const URL_RE = /^https?:\/\/\S+$/;

/**
 * One of 我的配置: name and link, 应用分流 with 其余流量 (兜底出口), 自定义规则, then behind 高级 the 策略组 and the raw
 * JSON. 检查 asks the engine; saving does too (f50d refuses a config that does not check).
 */
export function ConfigEditor({
  profile,
  config,
}: {
  profile: Profile;
  config: Config;
}) {
  const navigate = useNavigate();
  const save = useSaveConfig();
  const remove = useDeleteConfig();
  const set = useSetProxy();
  const [draft, setDraft] = useState(config.text);
  const [name, setName] = useState(config.name);
  const savedUrl = config.url ?? "";
  const [url, setUrl] = useState(savedUrl);
  const [refreshing, setRefreshing] = useState(false);
  const [checking, setChecking] = useState(false);
  const [checked, setChecked] = useState<RulesInfo | null>(null);
  const [more, setMore] = useState(() => {
    const p = parseDoc(config.text);
    return !p.ok || Boolean(p.doc.groups?.length);
  });
  const parsed = parseDoc(draft);
  const doc: RouteDoc = parsed.ok ? parsed.doc : {};
  const setDoc = (next: RouteDoc) => {
    setDraft(formatDoc(next));
    setChecked(null);
  };
  const rules = useRulesText(doc.rules ?? [], (r) =>
    setDoc({ ...doc, rules: r }),
  );
  const urlDirty = url.trim() !== savedUrl;
  const urlInvalid = Boolean(url.trim()) && !URL_RE.test(url.trim());
  const dirty =
    !sameDoc(draft, config.text) || name.trim() !== config.name || urlDirty;
  const inUse = config.id === profile.active_config;
  const close = () => navigate("/proxy/rules");

  const nodeNames = allNodes(profile)
    .filter((n) => !n.problem)
    .map((n) => n.name);
  const groups = doc.groups ?? [];
  const groupNames = groups.map((g) => g.name);
  const invalid = parsed.ok && Boolean(rules.error);
  const canSave = dirty && !invalid && !urlInvalid && !!name.trim();

  const onSave = () => {
    if (!canSave) return;
    save.mutate(
      {
        id: config.id,
        name: name.trim(),
        text: docForSave(draft),
        ...(url.trim() ? { url: url.trim() } : {}),
      },
      {
        onSuccess: (c) => {
          setDraft(c.text);
          const warnings = c.info?.warnings ?? [];
          if (warnings.length)
            toast.warning(t("proxy.rules.savedWarn"), {
              description: warnings.join(t("proxy.listSep")),
            });
          else
            toast.success(
              inUse && profile.routing === "rule"
                ? t("proxy.rules.savedLive")
                : t("proxy.rules.saved"),
            );
        },
      },
    );
  };

  const check = async () => {
    setChecking(true);
    try {
      setChecked(await checkRules(docForSave(draft)));
    } catch (e) {
      toast.error(t("proxy.rules.checkFailed"), {
        description: errorMessage(e),
      });
    } finally {
      setChecking(false);
    }
  };

  // what comes from the link replaces the text; saved like any edit
  const refresh = async () => {
    setRefreshing(true);
    try {
      const { text } = await fetchConfig(url.trim());
      setDraft(text);
      setChecked(null);
      toast.success(t("proxy.rules.fetched"));
    } catch (e) {
      toast.error(t("proxy.rules.fetchFailed"), {
        description: errorMessage(e),
      });
    } finally {
      setRefreshing(false);
    }
  };

  const formatJson = () => {
    try {
      setDraft(`${compactJson(JSON.parse(draft))}\n`);
    } catch {
      toast.error(t("proxy.rules.jsonBad"));
    }
  };

  // apps and final that pointed at a deleted group fall back to the current node
  const removeGroup = (i: number) => {
    const gone = groups[i].name;
    setDoc({
      ...doc,
      groups: groups.filter((_, j) => j !== i),
      apps: doc.apps?.map((a) =>
        a.outbound === gone ? { ...a, outbound: "proxy" } : a,
      ),
      final: doc.final === gone ? "proxy" : doc.final,
    });
  };
  const renameGroup = (i: number, from: string, to: string) => {
    const next = renameOutbound(doc, from, to);
    setDoc({
      ...next,
      groups: groups.map((g, j) => (j === i ? { ...g, name: to } : g)),
    });
  };

  const info = checked ?? (dirty ? null : config.info);
  const warnings = info?.warnings ?? [];

  return (
    <div className="flex flex-col gap-6">
      <PageHeader
        title={config.name}
        description={t("proxy.rules.editorDescription")}
        actions={
          <>
            <Button variant="outline" size="sm" onClick={close}>
              <ArrowLeftIcon data-icon="inline-start" />
              {t("common.action.back")}
            </Button>
            {dirty && (
              <Badge variant="secondary">{t("proxy.rules.unsaved")}</Badge>
            )}
            <PendingButton
              variant="outline"
              size="sm"
              pending={checking}
              disabled={!parsed.ok && !draft.trim()}
              onClick={() => void check()}
            >
              <ListChecksIcon data-icon="inline-start" />
              {t("proxy.rules.check.run")}
            </PendingButton>
            <PendingButton
              size="sm"
              pending={save.isPending}
              disabled={!canSave}
              onClick={onSave}
            >
              {t("common.action.save")}
            </PendingButton>
          </>
        }
      />
      <div className="flex max-w-4xl flex-col gap-6">
        <Card>
          <CardContent>
            <FieldGroup>
              <div className="grid gap-4 sm:grid-cols-2">
                <Field data-invalid={!name.trim() || undefined}>
                  <FieldLabel htmlFor="cfg-name">
                    {t("proxy.rules.name")}
                  </FieldLabel>
                  <Input
                    id="cfg-name"
                    value={name}
                    aria-invalid={!name.trim() || undefined}
                    onChange={(e) => setName(e.target.value)}
                  />
                </Field>
                <Field data-invalid={urlInvalid || undefined}>
                  <FieldLabel htmlFor="cfg-url">
                    {t("proxy.rules.link")}
                  </FieldLabel>
                  <div className="flex gap-2">
                    <Input
                      id="cfg-url"
                      value={url}
                      placeholder={t("proxy.rules.linkOptional")}
                      inputMode="url"
                      spellCheck={false}
                      autoCapitalize="off"
                      aria-invalid={urlInvalid || undefined}
                      className="font-mono text-xs"
                      onChange={(e) => setUrl(e.target.value)}
                    />
                    {url.trim() && !urlInvalid && (
                      <PendingButton
                        variant="outline"
                        pending={refreshing}
                        onClick={() => void refresh()}
                        aria-label={t("proxy.rules.refresh")}
                        title={t("proxy.rules.refreshHint")}
                      >
                        <RefreshCwIcon />
                      </PendingButton>
                    )}
                  </div>
                </Field>
              </div>
              <div className="flex flex-wrap items-center justify-between gap-2 text-sm">
                <span className="text-muted-foreground">
                  {checkSummary(draft, info)}
                  {config.updated_at &&
                    ` · ${t("proxy.rules.changed", { ago: formatAgo(config.updated_at) })}`}
                </span>
                {inUse ? (
                  <Badge variant="secondary" className="gap-1">
                    <CheckIcon />
                    {t("proxy.rules.inUse")}
                  </Badge>
                ) : (
                  <PendingButton
                    size="sm"
                    variant="outline"
                    pending={set.isPending}
                    onClick={() =>
                      set.mutate(
                        { active_config: config.id },
                        {
                          onSuccess: () =>
                            toast.success(
                              t("proxy.rules.switched", { name: config.name }),
                            ),
                        },
                      )
                    }
                  >
                    {t("proxy.rules.use")}
                  </PendingButton>
                )}
              </div>
            </FieldGroup>
          </CardContent>
        </Card>

        {warnings.length > 0 && (
          <Alert>
            <CircleAlertIcon />
            <AlertTitle>{t("proxy.rules.check.warnings")}</AlertTitle>
            <AlertDescription>
              <ul className="list-disc pl-4">
                {warnings.map((w) => (
                  <li key={w}>{w}</li>
                ))}
              </ul>
            </AlertDescription>
          </Alert>
        )}
        {checked && !warnings.length && (
          <Alert>
            <CheckIcon />
            <AlertTitle>{t("proxy.rules.check.ok")}</AlertTitle>
            <AlertDescription>{checkSummary(draft, checked)}</AlertDescription>
          </Alert>
        )}

        {parsed.ok ? (
          <>
            <AppsSection
              apps={doc.apps ?? []}
              onChange={(apps) => setDoc({ ...doc, apps })}
              groups={groupNames}
              nodes={nodeNames}
              final={
                <div className="flex items-center justify-between gap-3">
                  <span className="flex min-w-0 flex-col gap-0.5">
                    <span className="text-sm font-medium">
                      {t("proxy.rules.final")}
                    </span>
                    <span className="text-xs text-muted-foreground">
                      {t("proxy.rules.finalHint")}
                    </span>
                  </span>
                  <OutboundSelect
                    label={t("proxy.rules.final")}
                    value={doc.final ?? "proxy"}
                    onChange={(final) => setDoc({ ...doc, final })}
                    groups={groupNames}
                    nodes={nodeNames}
                    block={false}
                  />
                </div>
              }
            />
            <CustomRules rules={rules} groups={groupNames} nodes={nodeNames} />
            <div>
              <Button
                variant="outline"
                size="sm"
                onClick={() => setMore((m) => !m)}
              >
                <SlidersHorizontalIcon data-icon="inline-start" />
                {more
                  ? t("proxy.rules.lessAdvanced")
                  : t("proxy.rules.advanced")}
              </Button>
            </div>
            {more && (
              <GroupsSection
                groups={groups}
                nodes={nodeNames}
                onChange={(g) => setDoc({ ...doc, groups: g })}
                onRename={renameGroup}
                onRemove={removeGroup}
              />
            )}
          </>
        ) : (
          <Alert variant="destructive">
            <CircleAlertIcon />
            <AlertTitle>{t("proxy.rules.rawOnly")}</AlertTitle>
            <AlertDescription>{parsed.reason}</AlertDescription>
          </Alert>
        )}

        {(more || !parsed.ok) && (
          <Card>
            <CardHeader>
              <CardTitle>{t("proxy.rules.json")}</CardTitle>
              <CardDescription>{t("proxy.rules.jsonHint")}</CardDescription>
              <CardAction className="flex gap-1">
                <Button size="sm" variant="ghost" onClick={formatJson}>
                  {t("proxy.rules.format")}
                </Button>
                {draft !== config.text && (
                  <Button
                    size="sm"
                    variant="ghost"
                    onClick={() => {
                      setDraft(config.text);
                      setChecked(null);
                    }}
                  >
                    {t("common.action.reset")}
                  </Button>
                )}
              </CardAction>
            </CardHeader>
            <CardContent>
              <Textarea
                aria-label={t("proxy.rules.json")}
                value={draft}
                spellCheck={false}
                autoCapitalize="off"
                onChange={(e) => {
                  setDraft(e.target.value);
                  setChecked(null);
                }}
                aria-invalid={!parsed.ok || undefined}
                className="max-h-[32rem] min-h-64 font-mono text-xs leading-relaxed whitespace-pre"
              />
            </CardContent>
          </Card>
        )}

        <div className="flex justify-end">
          <ConfirmButton
            size="sm"
            title={t("proxy.rules.deleteTitle")}
            description={t("proxy.rules.deleteBody", { name: config.name })}
            confirmText={t("common.action.delete")}
            pending={remove.isPending}
            onConfirm={() => remove.mutate(config.id, { onSuccess: close })}
          >
            {t("proxy.rules.delete")}
          </ConfirmButton>
        </div>
      </div>
    </div>
  );
}
