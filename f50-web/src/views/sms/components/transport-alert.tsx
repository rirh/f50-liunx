import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert";
import { t } from "@/i18n";
import { TriangleAlertIcon } from "lucide-react";
import type { SmsTransport } from "../types";

const state = (v: boolean | null) =>
  v === null
    ? t("sms.transport.unknown")
    : v
      ? t("sms.transport.up")
      : t("sms.transport.down");

/**
 * Shown when the last sync found no SMS path at all (`transport.ready === false`): otherwise the inbox just stays
 * empty and sends fail with a bare modem error.
 */
export function TransportAlert({
  transport,
}: {
  transport: SmsTransport | null | undefined;
}) {
  if (transport?.ready !== false) return null;
  return (
    <Alert
      variant="destructive"
      className="mx-3 mb-2 w-auto border-0 bg-destructive/10"
    >
      <TriangleAlertIcon />
      <AlertTitle>{t("sms.transport.title")}</AlertTitle>
      <AlertDescription>
        {t("sms.transport.description", {
          ims: state(transport.ims),
          cs: state(transport.cs),
          nas: state(transport.nas),
        })}
      </AlertDescription>
    </Alert>
  );
}
