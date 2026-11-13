import { AnimatedNumber } from "@/components/f50/animated-number";
import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from "@/components/ui/tooltip";
import { t } from "@/i18n";
import type { SignalPoint } from "@/lib/series";
import { cn } from "@/lib/utils";

const BAR_COUNT = 24;

/** Five tones, best to worst (Apple's system green, mint, yellow, orange, red). */
const TONES = [
  "bg-[#34C759]",
  "bg-[#30D158]/70",
  "bg-[#FFCC00]",
  "bg-[#FF9500]",
  "bg-[#FF3B30]",
] as const;

interface Kind {
  label: string;
  key: "rsrp" | "sinr";
  unit: string;
  /** The value range the bar height spans. */
  floor: number;
  ceil: number;
  /** Lower bounds of tones 1-4; below the last is tone 5. */
  steps: [number, number, number, number];
}

const KINDS: Record<"rsrp" | "sinr", Kind> = {
  rsrp: {
    label: "RSRP",
    key: "rsrp",
    unit: "dBm",
    floor: -125,
    ceil: -70,
    steps: [-85, -95, -105, -115],
  },
  sinr: {
    label: "SINR",
    key: "sinr",
    unit: "dB",
    floor: -5,
    ceil: 30,
    steps: [20, 13, 5, 0],
  },
};

const toneOf = (k: Kind, v: number) => {
  const i = k.steps.findIndex((s) => v >= s);
  return TONES[i === -1 ? 4 : i];
};

const clock = (ms: number) =>
  new Date(ms).toLocaleTimeString(undefined, {
    hour12: false,
    hour: "2-digit",
    minute: "2-digit",
    second: "2-digit",
  });

/** The latest reading over the last 24, each bar as tall and as coloured as the reading is good. */
function SignalCell({ kind, points }: { kind: Kind; points: SignalPoint[] }) {
  const recent = points.slice(-BAR_COUNT);
  const latest = recent.at(-1)?.[kind.key] ?? null;
  const format = (n: number | null) =>
    n == null ? "—" : `${n.toFixed(1)} ${kind.unit}`;
  const pad = BAR_COUNT - recent.length;
  return (
    <div className="group/panel flex min-w-0 flex-col gap-1">
      <div className="flex items-baseline justify-between gap-2">
        <span className="text-xs text-muted-foreground">{kind.label}</span>
        <span className="tabular-nums text-sm font-semibold text-foreground">
          {latest == null ? (
            <span className="font-normal text-muted-foreground">
              {t("dashboard.device.probing")}
            </span>
          ) : (
            <AnimatedNumber value={latest} format={format} />
          )}
        </span>
      </div>
      <div
        className="grid h-3.5 shrink-0 items-end gap-[2px]"
        style={{ gridTemplateColumns: `repeat(${BAR_COUNT}, minmax(0, 1fr))` }}
      >
        {Array.from({ length: pad }, (_, i) => (
          <span
            key={`pad-${i}`}
            aria-hidden
            className="block h-1/5 w-full rounded-full bg-foreground/[0.07]"
          />
        ))}
        {recent.map((p) => {
          const v = p[kind.key];
          const height =
            v == null
              ? 20
              : 20 +
                Math.min(
                  1,
                  Math.max(0, (v - kind.floor) / (kind.ceil - kind.floor)),
                ) *
                  80;
          const tip = `${clock(p.t)}\n${v == null ? t("dashboard.device.noSample") : format(v)}`;
          return (
            <Tooltip key={p.t}>
              <TooltipTrigger asChild>
                <span
                  className="group/bar flex h-full w-full items-end"
                  aria-label={tip}
                >
                  <span
                    className={cn(
                      "block w-full rounded-full transition-opacity duration-150",
                      "group-hover/panel:opacity-50 group-hover/bar:opacity-100!",
                      v == null ? "bg-foreground/[0.07]" : toneOf(kind, v),
                    )}
                    style={{ height: `${height}%` }}
                  />
                </span>
              </TooltipTrigger>
              <TooltipContent side="top" sideOffset={6}>
                <span className="text-center whitespace-pre-line tabular-nums">
                  {tip}
                </span>
              </TooltipContent>
            </Tooltip>
          );
        })}
      </div>
    </div>
  );
}

export function SignalGrid({ points }: { points: SignalPoint[] }) {
  return (
    <div className="grid grid-cols-2 gap-x-5">
      <SignalCell kind={KINDS.rsrp} points={points} />
      <SignalCell kind={KINDS.sinr} points={points} />
    </div>
  );
}
