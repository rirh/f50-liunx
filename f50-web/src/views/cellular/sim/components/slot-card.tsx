import { useState } from "react";
import { KvList } from "@/components/f50/kv-list";
import { PendingButton } from "@/components/f50/pending-button";
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert";
import { Badge } from "@/components/ui/badge";
import {
  Card,
  CardContent,
  CardDescription,
  CardFooter,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import { Field, FieldDescription, FieldLabel } from "@/components/ui/field";
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group";
import { reportFormError } from "@/lib/form-error";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { JobSlot } from "../../shared/job-slot";
import { useJobSlot } from "../../shared/use-job-slot";
import { selectSlot, simKeys } from "../api";
import type { SimState, SimStatus } from "../types";

const STATE_LABEL: Record<SimState, string> = {
  ready: "就绪",
  absent: "未插卡",
  locked: "需要 PIN/PUK",
};
const SLOT_LABEL = { 1: "SIM1（外置卡槽）", 2: "SIM2（内置卡）" } as const;

/** The slot in use and the one the next boot takes; a selection only applies after a reboot. */
export function SlotCard({ sim }: { sim: SimStatus }) {
  const qc = useQueryClient();
  const [want, setWant] = useState<string>(String(sim.default));
  const { job, start } = useJobSlot();
  const select = useMutation({
    mutationFn: selectSlot,
    onSuccess: (r) => start(r.job, "保存启动卡槽"),
    onError: (e) => reportFormError(e),
  });
  const target = Number(want) as 1 | 2;
  return (
    <Card>
      <CardHeader>
        <CardTitle>卡槽选择</CardTitle>
        <CardDescription>
          F50 在启动时选定 SIM 卡，更改在下次重启后生效
        </CardDescription>
      </CardHeader>
      <CardContent className="flex flex-col gap-4">
        <KvList
          items={[
            { label: "当前使用", value: SLOT_LABEL[sim.active] },
            {
              label: "卡状态",
              value: sim.state ? (
                <Badge
                  variant={sim.state === "ready" ? "secondary" : "destructive"}
                >
                  {STATE_LABEL[sim.state]}
                </Badge>
              ) : (
                "未检测"
              ),
            },
            { label: "下次启动", value: SLOT_LABEL[sim.default] },
          ]}
        />
        {sim.reboot_required && (
          <Alert>
            <AlertTitle>重启后切换</AlertTitle>
            <AlertDescription>
              下次启动将使用 {SLOT_LABEL[sim.default]}，在此之前继续使用{" "}
              {SLOT_LABEL[sim.active]}。
            </AlertDescription>
          </Alert>
        )}
        <Field>
          <FieldLabel>下次启动使用</FieldLabel>
          <ToggleGroup
            type="single"
            variant="outline"
            value={want}
            onValueChange={(v) => v && setWant(v)}
            disabled={select.isPending}
          >
            <ToggleGroupItem value="1">SIM1 外置</ToggleGroupItem>
            <ToggleGroupItem value="2" disabled={!sim.slot2_supported}>
              SIM2 内置
            </ToggleGroupItem>
          </ToggleGroup>
          <FieldDescription>
            仅保存选择；需要手动重启设备才会切换。
          </FieldDescription>
        </Field>
        <JobSlot
          job={job}
          onDone={() => qc.invalidateQueries({ queryKey: simKeys.get })}
        />
      </CardContent>
      <CardFooter>
        <PendingButton
          pending={select.isPending}
          disabled={target === sim.default || sim.busy}
          onClick={() => select.mutate(target)}
        >
          保存
        </PendingButton>
      </CardFooter>
    </Card>
  );
}
