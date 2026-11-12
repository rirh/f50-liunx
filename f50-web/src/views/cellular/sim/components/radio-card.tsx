import { Alert, AlertDescription } from "@/components/ui/alert";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import {
  Field,
  FieldContent,
  FieldDescription,
  FieldGroup,
  FieldLabel,
  FieldSeparator,
} from "@/components/ui/field";
import { Switch } from "@/components/ui/switch";
import { reportFormError } from "@/lib/form-error";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { toast } from "sonner";
import { JobSlot } from "../../shared/job-slot";
import { useJobSlot } from "../../shared/use-job-slot";
import { setCardRadio } from "../api";
import type { CardSlot, SimStatus } from "../types";

/** Each card's radio setting, kept across reboots; both off is the persistent airplane mode. */
export function CardRadioCard({ sim }: { sim: SimStatus }) {
  const qc = useQueryClient();
  const { job, start } = useJobSlot();
  const refresh = () => qc.invalidateQueries({ queryKey: ["cellular"] });
  const card = useMutation({
    mutationFn: ({ slot, on }: { slot: CardSlot; on: boolean }) =>
      setCardRadio(slot, on),
    onSuccess: (r, { on }) => {
      if (r.job) start(r.job, on ? "开启射频" : "关闭射频");
      else toast.success("已保存，该卡下次启用时生效");
      refresh();
    },
    onError: (e) => reportFormError(e),
  });
  const disabled = sim.busy || card.isPending;
  const pendingSlot = card.isPending ? card.variables.slot : null;
  const rows: {
    slot: CardSlot;
    label: string;
    desc: string;
    checked: boolean;
  }[] = [
    {
      slot: "all",
      label: "飞行模式",
      desc: "关闭全部卡的射频，重启后保持",
      checked: sim.radio.all_off,
    },
    ...([1, 2] as const).map((n) => ({
      slot: String(n) as CardSlot,
      label: `SIM${n} 射频${sim.active === n ? "（使用中）" : ""}`,
      desc: sim.active === n ? "立即生效" : "保存，下次使用该卡时生效",
      checked: sim.radio.enabled[n - 1],
    })),
  ];
  return (
    <Card>
      <CardHeader>
        <CardTitle>射频开关</CardTitle>
        <CardDescription>每张卡的射频设置单独保存</CardDescription>
      </CardHeader>
      <CardContent className="flex flex-col gap-4">
        {sim.busy && (
          <Alert>
            <AlertDescription>
              射频操作进行中，完成后可再更改。
            </AlertDescription>
          </Alert>
        )}
        <FieldGroup>
          {rows.map((r, i) => (
            <div key={r.slot} className="contents">
              {i === 1 && <FieldSeparator />}
              <Field orientation="horizontal" data-disabled={disabled}>
                <FieldContent>
                  <FieldLabel htmlFor={`sim-radio-${r.slot}`}>
                    {r.label}
                  </FieldLabel>
                  <FieldDescription>
                    {pendingSlot === r.slot ? "正在保存…" : r.desc}
                  </FieldDescription>
                </FieldContent>
                <Switch
                  id={`sim-radio-${r.slot}`}
                  checked={r.checked}
                  disabled={disabled}
                  aria-busy={pendingSlot === r.slot}
                  onCheckedChange={(v) =>
                    card.mutate({ slot: r.slot, on: r.slot === "all" ? !v : v })
                  }
                />
              </Field>
            </div>
          ))}
        </FieldGroup>
        <JobSlot job={job} onDone={refresh} />
      </CardContent>
    </Card>
  );
}
