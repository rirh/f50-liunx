import { ConfirmButton } from "@/components/f50/confirm-button";
import { PendingButton } from "@/components/f50/pending-button";
import { Badge } from "@/components/ui/badge";
import {
  Card,
  CardContent,
  CardDescription,
  CardFooter,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import {
  Field,
  FieldDescription,
  FieldError,
  FieldGroup,
  FieldLabel,
} from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group";
import { reportFormError } from "@/lib/form-error";
import { zodResolver } from "@hookform/resolvers/zod";
import { LocateFixedIcon, XIcon } from "lucide-react";
import { Controller, useForm, useWatch } from "react-hook-form";
import { z } from "zod";
import { type OnJob, useLockSet } from "../use-lock-set";
import { ARFCN_RE, PCI_RE, cellValue, parseCell } from "../values";

const schema = z.object({
  rat: z.enum(["lte", "nr"]),
  arfcn: z.string().regex(ARFCN_RE, "1–7 位数字"),
  pci: z.string().regex(PCI_RE, "1–4 位数字"),
});
type Values = z.infer<typeof schema>;

const RAT_NAME = { lte: "LTE", nr: "NR" } as const;

/** Cell locks: one cell (RAT, ARFCN, PCI) added at a time, the serving cell, or removed per RAT. */
export function CellCard({
  cells,
  disabled,
  onJob,
}: {
  cells: string[];
  disabled: boolean;
  onJob: OnJob;
}) {
  const form = useForm<Values>({
    resolver: zodResolver(schema),
    defaultValues: { rat: "nr", arfcn: "", pci: "" },
  });
  const add = useLockSet(onJob);
  const quick = useLockSet(onJob);
  const parsed = cells.map(parseCell).filter((c) => c !== null);
  const hasNr = parsed.some((c) => c.rat === "nr");
  const hasLte = parsed.some((c) => c.rat === "lte");
  const busy = disabled || add.isPending || quick.isPending;
  const run = (value: string, title: string) =>
    quick.mutate(
      { kind: "cell", value, title },
      { onError: (e) => reportFormError(e) },
    );
  const quickPending = (v: string) =>
    quick.isPending && quick.variables.value === v;

  const submit = form.handleSubmit((v) => {
    const value = cellValue(v.rat, v.arfcn, v.pci);
    add.mutate(
      {
        kind: "cell",
        value,
        title: `锁定小区 ${RAT_NAME[v.rat]} ${v.arfcn} / PCI ${v.pci}`,
      },
      { onError: (e) => reportFormError(e, form.setError, { value: "arfcn" }) },
    );
  });
  const err = form.formState.errors;
  const rat = useWatch({ control: form.control, name: "rat" });

  return (
    <Card>
      <CardHeader>
        <CardTitle>小区锁定</CardTitle>
        <CardDescription>
          只驻留在指定小区；重启后靠「启动时自动应用」恢复
        </CardDescription>
      </CardHeader>
      <CardContent className="flex flex-col gap-4">
        <div className="flex flex-wrap items-center gap-2 text-sm">
          <span className="text-muted-foreground">已锁定：</span>
          {parsed.length === 0 && <Badge variant="secondary">无</Badge>}
          {parsed.map((c) => (
            <Badge
              key={`${c.rat}:${c.arfcn},${c.pci}`}
              variant="outline"
              className="font-mono"
            >
              {RAT_NAME[c.rat]} {c.arfcn} / {c.pci}
            </Badge>
          ))}
        </div>
        <form id="cell-lock-form" onSubmit={submit}>
          <FieldGroup>
            <Field>
              <FieldLabel>制式</FieldLabel>
              <Controller
                control={form.control}
                name="rat"
                render={({ field }) => (
                  <ToggleGroup
                    type="single"
                    variant="outline"
                    value={field.value}
                    disabled={busy}
                    onValueChange={(v) => v && field.onChange(v)}
                  >
                    <ToggleGroupItem value="nr">NR (5G)</ToggleGroupItem>
                    <ToggleGroupItem value="lte">LTE (4G)</ToggleGroupItem>
                  </ToggleGroup>
                )}
              />
            </Field>
            <div className="grid grid-cols-2 gap-4">
              <Field data-invalid={!!err.arfcn}>
                <FieldLabel htmlFor="cell-arfcn">
                  {rat === "nr" ? "NR-ARFCN" : "EARFCN"}
                </FieldLabel>
                <Input
                  id="cell-arfcn"
                  inputMode="numeric"
                  autoComplete="off"
                  disabled={busy}
                  aria-invalid={!!err.arfcn}
                  {...form.register("arfcn")}
                />
                <FieldError errors={[err.arfcn]} />
              </Field>
              <Field data-invalid={!!err.pci}>
                <FieldLabel htmlFor="cell-pci">PCI</FieldLabel>
                <Input
                  id="cell-pci"
                  inputMode="numeric"
                  autoComplete="off"
                  disabled={busy}
                  aria-invalid={!!err.pci}
                  {...form.register("pci")}
                />
                <FieldError errors={[err.pci]} />
              </Field>
            </div>
            <FieldDescription>
              可在「信号与小区」页的服务小区或邻区中查到 ARFCN 与 PCI。
            </FieldDescription>
          </FieldGroup>
        </form>
      </CardContent>
      <CardFooter className="flex flex-wrap gap-2">
        <PendingButton
          type="submit"
          form="cell-lock-form"
          pending={add.isPending}
          disabled={busy}
        >
          添加锁定
        </PendingButton>
        <PendingButton
          variant="outline"
          pending={quickPending("auto")}
          disabled={busy}
          icon={<LocateFixedIcon data-icon="inline-start" />}
          onClick={() => run("auto", "锁定当前服务小区")}
        >
          锁定当前服务小区
        </PendingButton>
        {hasNr && (
          <PendingButton
            variant="outline"
            pending={quickPending("off-nr")}
            disabled={busy}
            onClick={() => run("off-nr", "解除 NR 小区锁定")}
          >
            解除 NR
          </PendingButton>
        )}
        {hasLte && (
          <PendingButton
            variant="outline"
            pending={quickPending("off-lte")}
            disabled={busy}
            onClick={() => run("off-lte", "解除 LTE 小区锁定")}
          >
            解除 LTE
          </PendingButton>
        )}
        <ConfirmButton
          variant="outline"
          title="解除全部小区锁定？"
          description="NR 与 LTE 的小区锁定都会解除，协议栈随后重启。"
          confirmText="解除"
          pending={quickPending("off")}
          disabled={busy || parsed.length === 0}
          onConfirm={() => run("off", "解除全部小区锁定")}
        >
          <XIcon data-icon="inline-start" />
          全部解除
        </ConfirmButton>
      </CardFooter>
    </Card>
  );
}
