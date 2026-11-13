import { PageHeader } from "@/components/f50/page-header";
import { QueryState } from "@/components/f50/query-state";
import { t } from "@/i18n";
import { useQuery } from "@tanstack/react-query";
import { getLeds, ledKey } from "./api";
import { LampTile } from "./components/lamp-tile";
import { SettingsList } from "./components/settings-list";
import type { LedSettings } from "./types";

/** The two lamps side by side, each card lit as brightly as its lamp. */
function Lamps({ data }: { data: LedSettings }) {
  return (
    <div className="grid gap-4 md:grid-cols-2">
      {data.lamps.map((lamp) => (
        <LampTile
          key={lamp}
          lamp={lamp}
          on={data[lamp]}
          brightness={
            lamp === "network" ? data.network_brightness : data.wifi_brightness
          }
          dimmable={data.dimmable.includes(lamp)}
        />
      ))}
    </div>
  );
}

export default function Page() {
  const query = useQuery({ queryKey: ledKey, queryFn: getLeds });
  return (
    <>
      <PageHeader
        title={t("system.leds.title")}
        description={t("system.leds.description")}
      />
      <QueryState query={query} skeletonRows={6}>
        {(data) => (
          <div className="flex flex-col gap-8">
            <Lamps data={data} />
            <SettingsList data={data} />
          </div>
        )}
      </QueryState>
    </>
  );
}
