import { useState } from "react";
import { ResponsiveDialog } from "@/components/f50/form-dialog";
import { Button } from "@/components/ui/button";
import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from "@/components/ui/tooltip";
import { type Key, t } from "@/i18n";
import { CircleAlertIcon } from "lucide-react";

type Section = { title: Key; points: Key[] };

/** The page's how-to, one short section per card, read top to bottom. */
const SECTIONS: Section[] = [
  {
    title: "cellular.locks.help.read.title",
    points: ["cellular.locks.help.read.p1", "cellular.locks.help.read.p2"],
  },
  {
    title: "cellular.locks.help.mode.title",
    points: ["cellular.locks.help.mode.p1", "cellular.locks.help.mode.p2"],
  },
  {
    title: "cellular.locks.help.bands.title",
    points: [
      "cellular.locks.help.bands.p1",
      "cellular.locks.help.bands.p2",
      "cellular.locks.help.bands.p3",
    ],
  },
  {
    title: "cellular.locks.help.cell.title",
    points: [
      "cellular.locks.help.cell.p1",
      "cellular.locks.help.cell.p2",
      "cellular.locks.help.cell.p3",
    ],
  },
  {
    title: "cellular.locks.help.effect.title",
    points: ["cellular.locks.help.effect.p1", "cellular.locks.help.effect.p2"],
  },
  {
    title: "cellular.locks.help.reset.title",
    points: ["cellular.locks.help.reset.p1"],
  },
];

/** The "!" in the page header: opens how the network locks are used. */
export function LocksHelp() {
  const [open, setOpen] = useState(false);
  const label = t("cellular.locks.help.button");
  return (
    <>
      <Tooltip>
        <TooltipTrigger asChild>
          <Button
            variant="outline"
            size="icon-sm"
            aria-label={label}
            onClick={() => setOpen(true)}
          >
            <CircleAlertIcon />
          </Button>
        </TooltipTrigger>
        <TooltipContent>{label}</TooltipContent>
      </Tooltip>
      <ResponsiveDialog
        open={open}
        onOpenChange={setOpen}
        title={t("cellular.locks.help.title")}
        description={t("cellular.locks.help.description")}
        size="md"
      >
        <ol className="flex flex-col gap-4">
          {SECTIONS.map((s, i) => (
            <li key={s.title} className="flex gap-3">
              <span className="flex size-6 shrink-0 items-center justify-center rounded-full bg-muted text-xs font-medium tabular-nums">
                {i + 1}
              </span>
              <div className="flex min-w-0 flex-col gap-1.5 pt-0.5">
                <h3 className="text-sm font-medium">{t(s.title)}</h3>
                <ul className="flex list-disc flex-col gap-1 ps-4 text-sm text-muted-foreground marker:text-muted-foreground/50">
                  {s.points.map((p) => (
                    <li key={p}>{t(p)}</li>
                  ))}
                </ul>
              </div>
            </li>
          ))}
        </ol>
      </ResponsiveDialog>
    </>
  );
}
