import { PageHeader } from "@/components/f50/page-header";
import { t } from "@/i18n";
import { CellsCard } from "./components/cells-card";
import { IdentityCard } from "./components/identity-card";
import { RadioCard } from "./components/radio-card";
import { StatusCard } from "./components/status-card";
import { TrendCard } from "./components/trend-card";

/**
 * Read first, act last: the live reading beside its recent trend (what the page is opened for), then the serving and
 * neighbour cells, and at the foot the rarely used radio controls and the private identifiers read on demand.
 */
export default function SignalPage() {
  return (
    <div className="flex flex-col gap-4">
      <PageHeader
        title={t("cellular.signal.title")}
        description={t("cellular.signal.description")}
      />
      <div className="grid items-start gap-4 lg:grid-cols-2">
        <StatusCard />
        <TrendCard />
      </div>
      <CellsCard />
      <div className="grid items-start gap-4 lg:grid-cols-2">
        <RadioCard />
        <IdentityCard />
      </div>
    </div>
  );
}
