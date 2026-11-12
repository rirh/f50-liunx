import { PageHeader } from "@/components/f50/page-header";
import { t } from "@/i18n";
import { CommandForm } from "./components/command-form";
import { OutputPane } from "./components/output-pane";

/** The guarded AT terminal over `f50.at`. */
export default function AtTerminalPage() {
  return (
    <div className="flex flex-col gap-4">
      <PageHeader
        title={t("tools.at.page.title")}
        description={t("tools.at.page.description")}
      />
      <CommandForm />
      <OutputPane />
    </div>
  );
}
