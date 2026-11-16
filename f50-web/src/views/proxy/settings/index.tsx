import { useState } from "react";
import { PageHeader } from "@/components/f50/page-header";
import { PendingButton } from "@/components/f50/pending-button";
import { QueryState } from "@/components/f50/query-state";
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
  FieldSeparator,
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
import { BypassDevices } from "./components/bypass-devices";
import { useProfile, useSetSettings } from "../queries";
import type { LogLevel, ProxySettings, SettingsArgs } from "../types";

const REFRESH_HOURS = [0, 1, 3, 6, 12, 24, 48, 72, 168];
const LOG_LEVELS: LogLevel[] = ["error", "warning", "info", "debug"];
const URL_RE = /^https?:\/\/\S+$/;

/** 设置: what the proxy catches, who it leaves alone, how it tests and refreshes, and how much it logs. */
export default function ProxySettingsPage() {
  const profile = useProfile();
  return (
    <div className="flex flex-col gap-6">
      <PageHeader
        title={t("proxy.settings.title")}
        description={t("proxy.settings.description")}
      />
      <QueryState query={profile} skeletonRows={8}>
        {(p) => (
          <div className="grid items-start gap-6 lg:grid-cols-2">
            <div className="flex min-w-0 flex-col gap-6">
              <General settings={p.settings} />
              <Addresses settings={p.settings} />
            </div>
            <BypassDevices macs={p.settings.bypass_macs} />
          </div>
        )}
      </QueryState>
    </div>
  );
}

function SwitchRow({
  id,
  label,
  hint,
  checked,
  disabled,
  onChange,
}: {
  id: string;
  label: string;
  hint: string;
  checked: boolean;
  disabled: boolean;
  onChange: (v: boolean) => void;
}) {
  return (
    <Field orientation="horizontal">
      <FieldContent>
        <FieldLabel htmlFor={id}>{label}</FieldLabel>
        <FieldDescription className="text-xs">{hint}</FieldDescription>
      </FieldContent>
      <Switch
        id={id}
        checked={checked}
        disabled={disabled}
        onCheckedChange={onChange}
      />
    </Field>
  );
}

/** Switches and choices apply as soon as they change. */
function General({ settings: s }: { settings: ProxySettings }) {
  const set = useSetSettings();
  const apply = (args: SettingsArgs) => set.mutate(args);
  const hours = (h: number) =>
    h === 0
      ? t("proxy.settings.refresh.never")
      : t("proxy.settings.refresh.every", { count: h });
  return (
    <Card>
      <CardHeader>
        <CardTitle>{t("proxy.settings.general")}</CardTitle>
        <CardDescription>{t("proxy.settings.generalHint")}</CardDescription>
      </CardHeader>
      <CardContent>
        <FieldGroup>
          <SwitchRow
            id="ps-router"
            label={t("proxy.settings.router.label")}
            hint={t("proxy.settings.router.hint")}
            checked={s.intercept_router}
            disabled={set.isPending}
            onChange={(v) => apply({ intercept_router: v })}
          />
          <FieldSeparator />
          <SwitchRow
            id="ps-failover"
            label={t("proxy.settings.failover.label")}
            hint={t("proxy.settings.failover.hint")}
            checked={s.auto_failover}
            disabled={set.isPending}
            onChange={(v) => apply({ auto_failover: v })}
          />
          <FieldSeparator />
          <Field orientation="horizontal">
            <FieldContent>
              <FieldLabel htmlFor="ps-refresh">
                {t("proxy.settings.refresh.label")}
              </FieldLabel>
              <FieldDescription className="text-xs">
                {t("proxy.settings.refresh.hint")}
              </FieldDescription>
            </FieldContent>
            <Select
              value={String(s.subscription_refresh_hours)}
              disabled={set.isPending}
              onValueChange={(v) =>
                apply({ subscription_refresh_hours: Number(v) })
              }
            >
              <SelectTrigger id="ps-refresh" className="w-32">
                <SelectValue>{hours(s.subscription_refresh_hours)}</SelectValue>
              </SelectTrigger>
              <SelectContent position="popper" align="end">
                {[...new Set([...REFRESH_HOURS, s.subscription_refresh_hours])]
                  .sort((a, b) => a - b)
                  .map((h) => (
                    <SelectItem key={h} value={String(h)}>
                      {hours(h)}
                    </SelectItem>
                  ))}
              </SelectContent>
            </Select>
          </Field>
          <FieldSeparator />
          <Field orientation="horizontal">
            <FieldContent>
              <FieldLabel htmlFor="ps-log">
                {t("proxy.settings.logLevel.label")}
              </FieldLabel>
              <FieldDescription className="text-xs">
                {t("proxy.settings.logLevel.hint")}
              </FieldDescription>
            </FieldContent>
            <Select
              value={s.log_level}
              disabled={set.isPending}
              onValueChange={(v) => apply({ log_level: v as LogLevel })}
            >
              <SelectTrigger id="ps-log" className="w-32">
                <SelectValue />
              </SelectTrigger>
              <SelectContent position="popper" align="end">
                {LOG_LEVELS.map((l) => (
                  <SelectItem key={l} value={l}>
                    {t(`proxy.data.logs.levels.${l}`)}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
          </Field>
        </FieldGroup>
      </CardContent>
    </Card>
  );
}

/** The two addresses: saved with their own button, after a check that they are http(s) URLs. */
function Addresses({ settings: s }: { settings: ProxySettings }) {
  return (
    <Card>
      <CardHeader>
        <CardTitle>{t("proxy.settings.addresses")}</CardTitle>
      </CardHeader>
      <CardContent>
        <FieldGroup>
          <UrlField
            id="ps-probe"
            label={t("proxy.settings.probe.label")}
            hint={t("proxy.settings.probe.hint")}
            value={s.probe_url}
            field="probe_url"
          />
          <UrlField
            id="ps-rulesets"
            label={t("proxy.settings.ruleSets.label")}
            hint={t("proxy.settings.ruleSets.hint")}
            value={s.rule_sets_url}
            field="rule_sets_url"
          />
        </FieldGroup>
      </CardContent>
    </Card>
  );
}

function UrlField({
  id,
  label,
  hint,
  value,
  field,
}: {
  id: string;
  label: string;
  hint: string;
  value: string;
  field: "probe_url" | "rule_sets_url";
}) {
  const set = useSetSettings();
  const [draft, setDraft] = useState(value);
  const ok = URL_RE.test(draft.trim());
  const dirty = draft.trim() !== value;
  return (
    <Field data-invalid={!ok || undefined}>
      <FieldLabel htmlFor={id}>{label}</FieldLabel>
      <div className="flex gap-2">
        <Input
          id={id}
          value={draft}
          inputMode="url"
          spellCheck={false}
          autoCapitalize="off"
          aria-invalid={!ok || undefined}
          className="font-mono text-xs"
          onChange={(e) => setDraft(e.target.value)}
        />
        <PendingButton
          variant="outline"
          pending={set.isPending}
          disabled={!ok || !dirty}
          onClick={() => set.mutate({ [field]: draft.trim() })}
        >
          {t("common.action.save")}
        </PendingButton>
      </div>
      <FieldDescription className="text-xs">{hint}</FieldDescription>
    </Field>
  );
}
