import { Navigate, useSearchParams } from "react-router-dom";
import { PageHeader } from "@/components/f50/page-header";
import { QueryState } from "@/components/f50/query-state";
import { t } from "@/i18n";
import { ConfigEditor } from "./components/config-editor";
import { ConfigList } from "./components/config-list";
import { isBuiltin } from "../lib/configs";
import { useProfile } from "../queries";

/** 分流: the configs (/proxy/rules) and one config's editor (/proxy/rules?id=c1); the default is read-only. */
export default function RulesPage() {
  const [params] = useSearchParams();
  const id = params.get("id");
  const profile = useProfile();
  return (
    <QueryState query={profile} skeletonRows={8}>
      {(p) => {
        if (id) {
          const config = p.configs.find((c) => c.id === id);
          if (!config || isBuiltin(config))
            return <Navigate to="/proxy/rules" replace />;
          return <ConfigEditor key={id} profile={p} config={config} />;
        }
        return (
          <div className="flex flex-col gap-6">
            <PageHeader
              title={t("proxy.rules.title")}
              description={t("proxy.rules.description")}
            />
            <ConfigList profile={p} />
          </div>
        );
      }}
    </QueryState>
  );
}
