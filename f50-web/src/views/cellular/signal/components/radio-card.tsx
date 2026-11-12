import { ConfirmButton } from "@/components/f50/confirm-button";
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
} from "@/components/ui/field";
import { Switch } from "@/components/ui/switch";
import { reportFormError } from "@/lib/form-error";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { toast } from "sonner";
import { JobSlot } from "../../shared/job-slot";
import { useJobSlot } from "../../shared/use-job-slot";
import { getStatus, restartModem, setRadio, signalKeys } from "../api";

/** The card in use's radio (kept across reboots) and the modem's protocol-stack restart. */
export function RadioCard() {
  const qc = useQueryClient();
  const status = useQuery({
    queryKey: signalKeys.status,
    queryFn: getStatus,
    refetchInterval: 2000,
  });
  const { job, start } = useJobSlot();
  const refresh = () => qc.invalidateQueries({ queryKey: ["cellular"] });

  const radio = useMutation({
    mutationFn: setRadio,
    onSuccess: (r, on) => {
      if (r.job) start(r.job, on ? "开启射频" : "关闭射频");
      else toast.success("已保存，下次使用该卡时生效");
      refresh();
    },
    onError: (e) => reportFormError(e),
  });
  const restart = useMutation({
    mutationFn: restartModem,
    onSuccess: (r) => start(r.job, "重启调制解调器"),
    onError: (e) => reportFormError(e),
  });

  const s = status.data;
  const busy = !s || s.restarting || radio.isPending || restart.isPending;
  return (
    <Card>
      <CardHeader>
        <CardTitle>射频控制</CardTitle>
        <CardDescription>
          当前 SIM 卡的射频开关会保存，重启后保持
        </CardDescription>
      </CardHeader>
      <CardContent className="flex flex-col gap-4">
        <FieldGroup>
          <Field orientation="horizontal" data-disabled={busy}>
            <FieldContent>
              <FieldLabel htmlFor="modem-radio">
                射频{s ? `（SIM${s.slot}）` : ""}
              </FieldLabel>
              <FieldDescription>
                {radio.isPending
                  ? "正在切换…"
                  : "关闭后该卡不再联网，移动数据随之断开"}
              </FieldDescription>
            </FieldContent>
            <Switch
              id="modem-radio"
              checked={s?.radio_allowed ?? false}
              disabled={busy}
              aria-busy={radio.isPending}
              onCheckedChange={(on) => radio.mutate(on)}
            />
          </Field>
          <Field orientation="horizontal">
            <FieldContent>
              <FieldLabel>重启调制解调器</FieldLabel>
              <FieldDescription>
                重启协议栈，蜂窝连接会中断约半分钟
              </FieldDescription>
            </FieldContent>
            <ConfirmButton
              size="sm"
              title="重启调制解调器？"
              description="协议栈将重启，蜂窝网络和移动数据会中断约半分钟，随后自动恢复。"
              confirmText="重启"
              pending={restart.isPending}
              disabled={busy || !s?.radio_allowed}
              onConfirm={() => restart.mutate()}
            >
              重启
            </ConfirmButton>
          </Field>
        </FieldGroup>
        <JobSlot job={job} onDone={refresh} />
      </CardContent>
    </Card>
  );
}
