import { useState } from "react";
import { PendingButton } from "@/components/f50/pending-button";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
  DialogTrigger,
} from "@/components/ui/dialog";
import { Field, FieldDescription, FieldLabel } from "@/components/ui/field";
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group";
import { errorMessage } from "@/lib/format";
import { useMutation } from "@tanstack/react-query";
import { LogOutIcon } from "lucide-react";
import { toast } from "sonner";
import { leaveNetwork } from "../api";
import type { LeaveArgs } from "../types";

type SavedChoice = "disable" | "keep" | "forget";

const CHOICES: Record<
  SavedChoice,
  { label: string; hint: string; args: LeaveArgs }
> = {
  disable: {
    label: "保留",
    hint: "保留该网络，但开机时不再自动连接",
    args: {},
  },
  keep: {
    label: "保留并自动连接",
    hint: "开机时仍自动加入（热点在本次断开后恢复）",
    args: { keep: true },
  },
  forget: {
    label: "删除",
    hint: "删除已保存的网络和密码",
    args: { forget: true },
  },
};

/** Leave the network: the hotspot comes back; what happens to the saved network is the user's choice. */
export function LeaveDialog({
  disabled,
  onStarted,
}: {
  disabled?: boolean;
  onStarted: (job: string) => void;
}) {
  const [open, setOpen] = useState(false);
  const [choice, setChoice] = useState<SavedChoice>("disable");
  const leave = useMutation({
    mutationFn: leaveNetwork,
    onSuccess: (r) => {
      setOpen(false);
      onStarted(r.job);
    },
    onError: (e) => toast.error(errorMessage(e)),
  });
  return (
    <Dialog open={open} onOpenChange={setOpen}>
      <DialogTrigger asChild>
        <Button size="sm" variant="outline" disabled={disabled}>
          <LogOutIcon data-icon="inline-start" />
          断开
        </Button>
      </DialogTrigger>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>断开 Wi-Fi 上网？</DialogTitle>
          <DialogDescription>
            断开后无线电交还给热点，设备改用移动数据上网。
          </DialogDescription>
        </DialogHeader>
        <Field>
          <FieldLabel>已保存的网络</FieldLabel>
          <ToggleGroup
            type="single"
            variant="outline"
            size="sm"
            value={choice}
            onValueChange={(v) => v && setChoice(v as SavedChoice)}
          >
            {(Object.keys(CHOICES) as SavedChoice[]).map((k) => (
              <ToggleGroupItem key={k} value={k}>
                {CHOICES[k].label}
              </ToggleGroupItem>
            ))}
          </ToggleGroup>
          <FieldDescription>{CHOICES[choice].hint}</FieldDescription>
        </Field>
        <DialogFooter>
          <Button
            variant="outline"
            onClick={() => setOpen(false)}
            disabled={leave.isPending}
          >
            取消
          </Button>
          <PendingButton
            variant="destructive"
            pending={leave.isPending}
            onClick={() => leave.mutate(CHOICES[choice].args)}
          >
            {leave.isPending ? "提交中" : "断开"}
          </PendingButton>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
