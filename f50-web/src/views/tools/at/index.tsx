import { PageHeader } from "@/components/f50/page-header";
import { CommandForm } from "./components/command-form";
import { OutputPane } from "./components/output-pane";

/** The guarded AT terminal over `f50.at`. */
export default function AtTerminalPage() {
  return (
    <div className="flex flex-col gap-4">
      <PageHeader
        title="AT 终端"
        description="直接向调制解调器发送 AT 命令。PIN 和密码不会写入历史记录。"
      />
      <CommandForm />
      <OutputPane />
    </div>
  );
}
