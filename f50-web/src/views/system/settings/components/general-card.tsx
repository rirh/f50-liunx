import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import { t } from "@/i18n";
import { HostnameSetting } from "./hostname-form";
import { PasswordSetting } from "./password-form";
import { TimezoneSetting } from "./timezone-form";
import type { SettingsInfo } from "../types";

/**
 * Hostname, time zone and root password as one read-only list. Each is set once and changed rarely, so each row has
 * its own Edit button opening a dialog instead of three forms sitting open on the page.
 */
export function GeneralCard({ info }: { info: SettingsInfo }) {
  return (
    <Card>
      <CardHeader>
        <CardTitle>{t("system.settings.general.title")}</CardTitle>
        <CardDescription>
          {t("system.settings.general.description")}
        </CardDescription>
      </CardHeader>
      <CardContent className="flex flex-col divide-y">
        <HostnameSetting current={info.hostname} />
        <TimezoneSetting current={info.timezone} time={info.time} />
        <PasswordSetting />
      </CardContent>
    </Card>
  );
}
