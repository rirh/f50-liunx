import { useState } from "react";
import { Link, Navigate, useNavigate, useSearchParams } from "react-router-dom";
import { ConfirmButton } from "@/components/f50/confirm-button";
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
import { FieldGroup } from "@/components/ui/field";
import { t } from "@/i18n";
import { copyText } from "@/lib/clipboard";
import { errorMessage } from "@/lib/format";
import { useQueryClient } from "@tanstack/react-query";
import { ArrowLeftIcon, CheckIcon, CopyIcon, GaugeIcon } from "lucide-react";
import { toast } from "sonner";
import { SelectField, TextField } from "./components/fields";
import { LocalSocksCard } from "./components/local-socks";
import {
  CredentialFields,
  SecurityFields,
  type Setter,
  TransportFields,
} from "./components/node-fields";
import { PasteLink } from "./components/paste-link";
import { ViaField } from "./components/via-field";
import { probeNode, proxyKeys, saveNode } from "../api";
import { allNodes, latencyTone } from "../lib/format";
import {
  buildLink,
  emptyForm,
  missingFields,
  type NodeForm,
  parseLink,
  PROTOCOLS,
  type Protocol,
  usesTransport,
} from "../lib/node-form";
import {
  useAddSubscription,
  useDeleteNode,
  useProfile,
  useSaveNode,
} from "../queries";
import type { InspectedNode, Profile } from "../types";

type EditorType = "subscription" | Protocol;
const URL_RE = /^https?:\/\/\S+$/;

/** /proxy/node (add), /proxy/node?type=subscription, /proxy/node?id=… (edit any node). */
export default function NodePage() {
  const [params] = useSearchParams();
  const id = params.get("id") ?? undefined;
  const profile = useProfile();
  return (
    <QueryState query={profile} skeletonRows={8}>
      {(p) => {
        const existing = id ? allNodes(p).find((n) => n.id === id) : undefined;
        if (id && !existing) return <Navigate to="/proxy" replace />;
        return (
          <NodeEditor
            key={id ?? params.get("type") ?? "new"}
            profile={p}
            id={id}
            startAsSubscription={params.get("type") === "subscription"}
          />
        );
      }}
    </QueryState>
  );
}

function NodeEditor({
  profile,
  id,
  startAsSubscription,
}: {
  profile: Profile;
  id?: string;
  startAsSubscription: boolean;
}) {
  const navigate = useNavigate();
  const qc = useQueryClient();
  const existing = id ? allNodes(profile).find((n) => n.id === id) : undefined;
  const isLocal = !!existing && profile.local_nodes.some((n) => n.id === id);
  const [form, setForm] = useState<NodeForm>(
    () => (existing && parseLink(existing.link)) || emptyForm(),
  );
  const [type, setType] = useState<EditorType>(
    existing
      ? form.protocol
      : startAsSubscription || !profile.subscriptions.length
        ? "subscription"
        : "vless",
  );
  const [subUrl, setSubUrl] = useState("");
  const [subName, setSubName] = useState("");
  const [via, setVia] = useState(existing?.via ?? "");
  const [copied, setCopied] = useState(false);
  const [latency, setLatency] = useState<number | "pending" | "fail" | null>(
    null,
  );
  const save = useSaveNode();
  const subscribe = useAddSubscription();
  const remove = useDeleteNode();
  const [bulk, setBulk] = useState(false);
  const close = () => navigate("/proxy");

  const set: Setter = (key) => (value) =>
    setForm((f) => ({ ...f, [key]: value }));
  const isSubscription = type === "subscription";
  const missing = missingFields(form);
  const link = missing.length ? null : buildLink(form, existing?.link);
  const dirty =
    !existing || link !== existing.link || via !== (existing.via ?? "");
  const subOk = URL_RE.test(subUrl.trim());
  const saving = save.isPending || subscribe.isPending || bulk;

  // switching the protocol keeps where the server is and resets the rest
  const changeType = (next: EditorType) => {
    setType(next);
    if (next !== "subscription" && next !== form.protocol)
      setForm((f) => ({
        ...emptyForm(next),
        name: f.name,
        server: f.server,
        port: f.port,
      }));
  };

  const submit = () => {
    if (isSubscription) {
      if (!subOk) return;
      subscribe.mutate(
        { url: subUrl.trim(), name: subName.trim() || undefined },
        { onSuccess: close },
      );
      return;
    }
    if (!link || !dirty) return;
    save.mutate(
      { id, link, name: form.name.trim() || undefined, via },
      {
        onSuccess: () => {
          toast.success(
            id ? t("proxy.toast.nodeSaved") : t("proxy.toast.nodeAdded"),
          );
          close();
        },
      },
    );
  };

  // one good link is saved at once; one with a problem fills the form; several all become local nodes
  const onNodes = async (nodes: InspectedNode[]) => {
    if (nodes.length === 1) {
      const [n] = nodes;
      const parsed = parseLink(n.link);
      if (n.problem || !parsed) {
        if (parsed) {
          setForm(parsed);
          setType(parsed.protocol);
        }
        toast.warning(t("proxy.paste.problem"), {
          description: n.problem ?? t("proxy.paste.cannotEdit"),
        });
        return;
      }
      save.mutate(
        { link: n.link, name: n.name || undefined },
        {
          onSuccess: () => {
            toast.success(t("proxy.toast.nodeAdded"));
            close();
          },
        },
      );
      return;
    }
    setBulk(true);
    const failed: string[] = [];
    for (const n of nodes) {
      if (n.problem) {
        failed.push(`${n.name}: ${n.problem}`);
        continue;
      }
      try {
        await saveNode({ link: n.link, name: n.name || undefined });
      } catch (e) {
        failed.push(errorMessage(e));
      }
    }
    setBulk(false);
    const added = nodes.length - failed.length;
    if (added) {
      toast.success(t("proxy.paste.added", { count: added }), {
        description: failed.length
          ? t("proxy.paste.someFailed", {
              count: failed.length,
              first: failed[0],
            })
          : undefined,
      });
      await qc.invalidateQueries({ queryKey: proxyKeys.profile });
      close();
    } else toast.error(t("proxy.paste.noneAdded"), { description: failed[0] });
  };

  const copyLink = async () => {
    const text = link ?? existing?.link;
    if (!text) return;
    try {
      await copyText(text);
      setCopied(true);
      setTimeout(() => setCopied(false), 1500);
      toast.success(t("proxy.node.copied"));
    } catch (e) {
      toast.error(t("proxy.node.copyFailed"), { description: errorMessage(e) });
    }
  };

  const test = async () => {
    if (!existing) return;
    setLatency("pending");
    try {
      setLatency((await probeNode(existing.id)).ms);
    } catch {
      setLatency("fail");
    }
  };

  const types: { value: EditorType; label: string }[] = [
    ...(id
      ? []
      : [
          {
            value: "subscription" as const,
            label: t("proxy.node.typeSubscription"),
          },
        ]),
    ...PROTOCOLS,
  ];
  const title = id
    ? t("proxy.node.editTitle")
    : isSubscription
      ? t("proxy.node.addSubscriptionTitle")
      : t("proxy.node.addTitle");
  const canSave = isSubscription ? subOk : !!link && dirty;

  return (
    <div className="flex flex-col gap-6">
      <PageHeader
        title={title}
        description={
          existing
            ? existing.name
            : isSubscription
              ? t("proxy.node.subscriptionDescription")
              : t("proxy.node.addDescription")
        }
        actions={
          <>
            <Button asChild variant="outline" size="sm">
              <Link to="/proxy">
                <ArrowLeftIcon data-icon="inline-start" />
                {t("common.action.back")}
              </Link>
            </Button>
            <PendingButton
              size="sm"
              pending={saving}
              disabled={!canSave}
              onClick={submit}
            >
              {t("common.action.save")}
            </PendingButton>
          </>
        }
      />
      <form
        className="flex max-w-3xl flex-col gap-6"
        onSubmit={(e) => {
          e.preventDefault();
          submit();
        }}
      >
        <Card>
          <CardContent>
            <FieldGroup>
              <SelectField
                label={t("proxy.node.type")}
                value={type}
                options={types}
                onChange={changeType}
                hint={
                  existing && !isLocal
                    ? t("proxy.node.fromSubscription")
                    : undefined
                }
              />
            </FieldGroup>
          </CardContent>
        </Card>

        {isSubscription ? (
          <Card>
            <CardHeader>
              <CardTitle>{t("proxy.node.typeSubscription")}</CardTitle>
              <CardDescription>
                {t("proxy.node.subscriptionHint")}
              </CardDescription>
            </CardHeader>
            <CardContent>
              <FieldGroup>
                <TextField
                  label={t("proxy.sub.url")}
                  value={subUrl}
                  onChange={setSubUrl}
                  placeholder="https://…"
                  inputMode="url"
                  invalid={!!subUrl && !subOk}
                  mono
                />
                <TextField
                  label={t("proxy.sub.name")}
                  value={subName}
                  onChange={setSubName}
                  placeholder={t("proxy.sub.namePlaceholder")}
                />
              </FieldGroup>
            </CardContent>
          </Card>
        ) : (
          <>
            {!existing && (
              <Card>
                <CardHeader>
                  <CardTitle>{t("proxy.paste.title")}</CardTitle>
                  <CardDescription>
                    {t("proxy.paste.description")}
                  </CardDescription>
                </CardHeader>
                <CardContent>
                  <PasteLink
                    onNodes={(n) => void onNodes(n)}
                    saving={save.isPending || bulk}
                  />
                </CardContent>
              </Card>
            )}
            <Card>
              <CardHeader>
                <CardTitle>{t("proxy.node.server")}</CardTitle>
                <CardDescription>
                  {missing.length
                    ? t("proxy.node.missing", {
                        fields: missing.join(t("proxy.listSep")),
                      })
                    : t("proxy.node.checked")}
                </CardDescription>
              </CardHeader>
              <CardContent>
                <FieldGroup>
                  <div className="grid gap-4 sm:grid-cols-[minmax(0,1fr)_8rem]">
                    <TextField
                      label={t("proxy.form.field.server")}
                      value={form.server}
                      onChange={set("server")}
                      placeholder={t("proxy.form.ph.server")}
                      invalid={!form.server.trim()}
                    />
                    <TextField
                      label={t("proxy.form.field.port")}
                      value={form.port}
                      onChange={(v) => set("port")(v.replace(/\D/g, ""))}
                      placeholder="1–65535"
                      inputMode="numeric"
                      invalid={
                        !(Number(form.port) >= 1 && Number(form.port) <= 65535)
                      }
                    />
                  </div>
                  <CredentialFields form={form} set={set} />
                  <TextField
                    label={t("proxy.form.field.name")}
                    value={form.name}
                    onChange={set("name")}
                    placeholder={t("proxy.form.ph.name")}
                  />
                </FieldGroup>
              </CardContent>
            </Card>
            {usesTransport(form.protocol) && (
              <>
                <Card>
                  <CardHeader>
                    <CardTitle>{t("proxy.node.transport")}</CardTitle>
                  </CardHeader>
                  <CardContent>
                    <TransportFields form={form} set={set} />
                  </CardContent>
                </Card>
                <Card>
                  <CardHeader>
                    <CardTitle>{t("proxy.node.security")}</CardTitle>
                  </CardHeader>
                  <CardContent>
                    <SecurityFields form={form} set={set} />
                  </CardContent>
                </Card>
              </>
            )}
            <Card>
              <CardHeader>
                <CardTitle>{t("proxy.node.via")}</CardTitle>
                <CardDescription>
                  {form.protocol === "hysteria2"
                    ? t("proxy.node.viaHintUdp")
                    : t("proxy.node.viaHint")}
                </CardDescription>
              </CardHeader>
              <CardContent>
                <ViaField
                  profile={profile}
                  id={id}
                  protocol={form.protocol}
                  value={via}
                  onChange={setVia}
                />
              </CardContent>
            </Card>
            {existing && <LocalSocksCard node={existing} />}
            {existing && (
              <Card>
                <CardContent className="flex flex-col gap-4">
                  {existing.warnings.length > 0 && (
                    <Alert>
                      <AlertTitle>{t("proxy.node.warnings")}</AlertTitle>
                      <AlertDescription>
                        <ul className="list-disc pl-4">
                          {existing.warnings.map((w) => (
                            <li key={w}>{w}</li>
                          ))}
                        </ul>
                      </AlertDescription>
                    </Alert>
                  )}
                  {existing.problem && (
                    <Alert variant="destructive">
                      <AlertTitle>{t("proxy.node.broken")}</AlertTitle>
                      <AlertDescription>{existing.problem}</AlertDescription>
                    </Alert>
                  )}
                  <div className="flex flex-wrap items-center gap-2">
                    <Button
                      type="button"
                      variant="outline"
                      size="sm"
                      disabled={latency === "pending" || !!existing.problem}
                      aria-busy={latency === "pending"}
                      onClick={() => void test()}
                    >
                      <GaugeIcon data-icon="inline-start" />
                      {t("proxy.node.test")}
                    </Button>
                    {typeof latency === "number" && (
                      <span
                        className={`text-sm tabular-nums ${latencyTone(latency)}`}
                      >
                        {latency} ms
                      </span>
                    )}
                    {latency === "fail" && (
                      <span className="text-sm text-destructive">
                        {t("proxy.node.timeout")}
                      </span>
                    )}
                    <Button
                      type="button"
                      variant="outline"
                      size="sm"
                      onClick={() => void copyLink()}
                    >
                      {copied ? (
                        <CheckIcon data-icon="inline-start" />
                      ) : (
                        <CopyIcon data-icon="inline-start" />
                      )}
                      {copied
                        ? t("common.action.copied")
                        : t("proxy.node.copyLink")}
                    </Button>
                    {isLocal && (
                      <ConfirmButton
                        size="sm"
                        className="ms-auto"
                        title={t("proxy.node.deleteTitle")}
                        description={t("proxy.node.deleteBody", {
                          name: existing.name,
                        })}
                        confirmText={t("common.action.delete")}
                        pending={remove.isPending}
                        onConfirm={() =>
                          remove.mutate(existing.id, { onSuccess: close })
                        }
                      >
                        {t("proxy.node.delete")}
                      </ConfirmButton>
                    )}
                  </div>
                  <p className="text-xs text-muted-foreground">
                    {isLocal
                      ? t("proxy.node.copyHint")
                      : t("proxy.node.subscriptionNodeHint")}
                  </p>
                </CardContent>
              </Card>
            )}
          </>
        )}
      </form>
    </div>
  );
}
