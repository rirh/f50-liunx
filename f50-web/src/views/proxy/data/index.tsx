import { useSearchParams } from "react-router-dom";
import { PageHeader } from "@/components/f50/page-header";
import { AnimatedSegmentedTabs } from "@/components/ui/animated-segmented-tabs";
import { t } from "@/i18n";
import { ConnectionsView } from "./components/connections-view";
import { LogsView } from "./components/logs-view";
import { TrafficView } from "./components/traffic-view";

type View = "traffic" | "connections" | "logs";
const VIEWS: View[] = ["traffic", "connections", "logs"];

/** 连接: 流量 · 连接 · 日志 (the view in the address, `?view=`). Each polls every second while it is shown. */
export default function ProxyDataPage() {
  const [params, setParams] = useSearchParams();
  const asked = params.get("view") as View | null;
  const view: View = asked && VIEWS.includes(asked) ? asked : "traffic";
  const show = (v: View) =>
    setParams(v === "traffic" ? {} : { view: v }, { replace: true });
  // full-bleed route: the page keeps the console's width and padding itself, so the log terminal can fill the height
  const header = (
    <PageHeader
      title={t("proxy.data.title")}
      description={t("proxy.data.description")}
      actions={
        <AnimatedSegmentedTabs
          label={t("proxy.data.title")}
          value={view}
          onValueChange={show}
          options={VIEWS.map((v) => ({
            value: v,
            label: t(`proxy.data.view.${v}`),
          }))}
          triggerClassName="px-3"
        />
      }
    />
  );
  if (view === "logs")
    return (
      <div className="mx-auto flex min-h-0 w-full max-w-6xl flex-1 flex-col gap-6 p-4 sm:p-6">
        {header}
        <LogsView />
      </div>
    );
  return (
    <div className="min-h-0 flex-1 overflow-y-auto">
      <div className="mx-auto flex w-full max-w-6xl flex-col gap-6 p-4 sm:p-6">
        {header}
        {view === "traffic" && (
          <TrafficView onConnections={() => show("connections")} />
        )}
        {view === "connections" && <ConnectionsView />}
      </div>
    </div>
  );
}
