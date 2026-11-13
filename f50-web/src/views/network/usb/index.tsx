import { PageHeader } from "@/components/f50/page-header";
import { PendingButton } from "@/components/f50/pending-button";
import { QueryState } from "@/components/f50/query-state";
import { t } from "@/i18n";
import { useQuery } from "@tanstack/react-query";
import { RefreshCwIcon } from "lucide-react";
import { getUsb, usbKeys } from "./api";
import { UsbAdaptersCard } from "./components/adapters-card";
import { UsbAdbCard } from "./components/adb-card";
import { UsbStatusCard } from "./components/status-card";

/**
 * The port's state beside adb, then the USB adapters across. The role and network settings open from the status card
 * in dialogs instead of taking two cards.
 */
export default function UsbPage() {
  const query = useQuery({ queryKey: usbKeys.info, queryFn: getUsb });
  return (
    <div className="flex flex-col gap-4">
      <PageHeader
        title="USB"
        description={t("network.usb.description")}
        actions={
          <PendingButton
            variant="outline"
            size="sm"
            pending={query.isFetching}
            onClick={() => query.refetch()}
          >
            <RefreshCwIcon data-icon="inline-start" />
            {query.isFetching
              ? t("common.action.refreshing")
              : t("common.action.refresh")}
          </PendingButton>
        }
      />
      <QueryState query={query} skeletonRows={8}>
        {(info) => (
          <div className="grid items-start gap-4 lg:grid-cols-2">
            <UsbStatusCard info={info} />
            <UsbAdbCard adb={info.adb} hostRole={info.role === "host"} />
            <div className="lg:col-span-2">
              <UsbAdaptersCard hostRole={info.role === "host"} />
            </div>
          </div>
        )}
      </QueryState>
    </div>
  );
}
