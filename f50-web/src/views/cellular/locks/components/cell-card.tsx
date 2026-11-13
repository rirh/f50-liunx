import { useMemo, useState } from "react";
import { ConfirmButton } from "@/components/f50/confirm-button";
import { FormDialog } from "@/components/f50/form-dialog";
import { PendingButton } from "@/components/f50/pending-button";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import {
  Card,
  CardAction,
  CardContent,
  CardDescription,
  CardFooter,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import {
  Field,
  FieldError,
  FieldGroup,
  FieldLabel,
} from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group";
import { t } from "@/i18n";
import { reportFormError } from "@/lib/form-error";
import { zodResolver } from "@hookform/resolvers/zod";
import { LocateFixedIcon, PlusIcon, XIcon } from "lucide-react";
import { Controller, useForm, useWatch } from "react-hook-form";
import { z } from "zod";
import { type OnJob, useLockSet } from "../use-lock-set";
import { ARFCN_RE, PCI_RE, cellValue, parseCell } from "../values";

const schema = () =>
  z.object({
    rat: z.enum(["lte", "nr"]),
    arfcn: z.string().regex(ARFCN_RE, t("cellular.locks.cell.digits7")),
    pci: z.string().regex(PCI_RE, t("cellular.locks.cell.digits4")),
  });
type Values = z.infer<ReturnType<typeof schema>>;

const RAT_NAME = { lte: "LTE", nr: "NR" } as const;

const EMPTY: Values = { rat: "nr", arfcn: "", pci: "" };

/**
 * Cell locks: the locked cells and the one-click actions (lock the serving cell, remove per RAT) on the page; a cell
 * by RAT, ARFCN and PCI is added one at a time in a dialog.
 */
export function CellCard({
  cells,
  disabled,
  onJob,
}: {
  cells: string[];
  disabled: boolean;
  onJob: OnJob;
}) {
  const [open, setOpen] = useState(false);
  const form = useForm<Values>({
    resolver: zodResolver(useMemo(() => schema(), [])),
    defaultValues: EMPTY,
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
        title: t("cellular.locks.cell.job", {
          rat: RAT_NAME[v.rat],
          arfcn: v.arfcn,
          pci: v.pci,
        }),
      },
      {
        onSuccess: () => setOpen(false),
        onError: (e) => reportFormError(e, form.setError, { value: "arfcn" }),
      },
    );
  });
  const err = form.formState.errors;
  const rat = useWatch({ control: form.control, name: "rat" });

  return (
    <Card>
      <CardHeader>
        <CardTitle>{t("cellular.locks.cell.title")}</CardTitle>
        <CardDescription>
          {t("cellular.locks.cell.description")}
        </CardDescription>
        <CardAction>
          <Button
            variant="outline"
            size="sm"
            disabled={busy}
            onClick={() => {
              form.reset(EMPTY);
              setOpen(true);
            }}
          >
            <PlusIcon data-icon="inline-start" />
            {t("cellular.locks.cell.add")}
          </Button>
        </CardAction>
      </CardHeader>
      <CardContent className="flex flex-col gap-4">
        <div className="flex flex-wrap items-center gap-2 text-sm">
          <span className="text-muted-foreground">
            {t("cellular.locks.cell.locked")}
          </span>
          {parsed.length === 0 && (
            <Badge variant="secondary">{t("common.state.none")}</Badge>
          )}
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
      </CardContent>
      <CardFooter className="flex flex-wrap gap-2">
        <PendingButton
          variant="outline"
          pending={quickPending("auto")}
          disabled={busy}
          icon={<LocateFixedIcon data-icon="inline-start" />}
          onClick={() => run("auto", t("cellular.locks.cell.serving"))}
        >
          {t("cellular.locks.cell.serving")}
        </PendingButton>
        {hasNr && (
          <PendingButton
            variant="outline"
            pending={quickPending("off-nr")}
            disabled={busy}
            onClick={() => run("off-nr", t("cellular.locks.cell.unlockNrJob"))}
          >
            {t("cellular.locks.cell.unlockNr")}
          </PendingButton>
        )}
        {hasLte && (
          <PendingButton
            variant="outline"
            pending={quickPending("off-lte")}
            disabled={busy}
            onClick={() =>
              run("off-lte", t("cellular.locks.cell.unlockLteJob"))
            }
          >
            {t("cellular.locks.cell.unlockLte")}
          </PendingButton>
        )}
        <ConfirmButton
          variant="outline"
          title={t("cellular.locks.cell.unlockAllTitle")}
          description={t("cellular.locks.cell.unlockAllDescription")}
          confirmText={t("cellular.locks.cell.unlock")}
          pending={quickPending("off")}
          disabled={busy || parsed.length === 0}
          onConfirm={() => run("off", t("cellular.locks.cell.unlockAllJob"))}
        >
          <XIcon data-icon="inline-start" />
          {t("cellular.locks.cell.unlockAll")}
        </ConfirmButton>
      </CardFooter>
      <FormDialog
        open={open}
        onOpenChange={setOpen}
        title={t("cellular.locks.cell.addTitle")}
        description={t("cellular.locks.cell.hint")}
        formId="cell-lock-form"
        pending={add.isPending}
        dirty={form.formState.isDirty}
        submitText={t("cellular.locks.cell.add")}
        size="sm"
      >
        <form id="cell-lock-form" onSubmit={submit}>
          <FieldGroup>
            <Field>
              <FieldLabel>{t("cellular.locks.cell.rat")}</FieldLabel>
              <Controller
                control={form.control}
                name="rat"
                render={({ field }) => (
                  <ToggleGroup
                    type="single"
                    variant="outline"
                    value={field.value}
                    disabled={add.isPending}
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
                  disabled={add.isPending}
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
                  disabled={add.isPending}
                  aria-invalid={!!err.pci}
                  {...form.register("pci")}
                />
                <FieldError errors={[err.pci]} />
              </Field>
            </div>
          </FieldGroup>
        </form>
      </FormDialog>
    </Card>
  );
}
