import { PageHeader } from "@/components/f50/page-header";
import { CellsCard } from "./components/cells-card";
import { IdentityCard } from "./components/identity-card";
import { RadioCard } from "./components/radio-card";
import { StatusCard } from "./components/status-card";

export default function SignalPage() {
  return (
    <div className="flex flex-col gap-4">
      <PageHeader
        title="信号与小区"
        description="注册状态、信号质量、服务小区与邻区（读数来自后台采集，不阻塞设备）"
      />
      <div className="grid gap-4 lg:grid-cols-2">
        <StatusCard />
        <div className="flex flex-col gap-4">
          <RadioCard />
          <IdentityCard />
        </div>
      </div>
      <CellsCard />
    </div>
  );
}
