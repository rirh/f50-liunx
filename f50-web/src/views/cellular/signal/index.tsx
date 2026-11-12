import { PageHeader } from "@/components/f50/page-header";
import { t } from "@/i18n";
import { CellsCard } from "./components/cells-card";
import { IdentityCard } from "./components/identity-card";
import { RadioCard } from "./components/radio-card";
import { StatusCard } from "./components/status-card";
import { TrendCard } from "./components/trend-card";

export default function SignalPage() {
  return (
    <div className="flex flex-col gap-4">
      <PageHeader
        title={t("cellular.signal.title")}
        description={t("cellular.signal.description")}
      />
      <div className="grid gap-4 lg:grid-cols-2">
        <StatusCard />
        <div className="flex flex-col gap-4">
          <RadioCard />
          <IdentityCard />
        </div>
      </div>
      <TrendCard />
      <CellsCard />
    </div>
  );
}
