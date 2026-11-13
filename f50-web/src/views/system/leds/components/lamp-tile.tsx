import { type CSSProperties, useState } from "react";
import { Slider } from "@/components/ui/slider";
import { type Key, t } from "@/i18n";
import { cn } from "@/lib/utils";
import {
  type LucideIcon,
  SignalIcon,
  SunDimIcon,
  SunIcon,
  WifiIcon,
} from "lucide-react";
import { AppleSwitch } from "./apple-controls";
import { useLedSet } from "./use-led-set";
import type { Lamp } from "../types";

const LAMP: Record<
  Lamp,
  { label: Key; description: Key; icon: LucideIcon; color: string }
> = {
  network: {
    label: "system.leds.lamps.network",
    description: "system.leds.lamps.networkText",
    icon: SignalIcon,
    color: "#0a84ff",
  },
  wifi: {
    label: "system.leds.lamps.wifi",
    description: "system.leds.lamps.wifiText",
    icon: WifiIcon,
    color: "#30b0c7",
  },
};

/**
 * One lamp as a card that glows like the lamp: the background carries the lamp's colour at the strength it is set to,
 * and follows the slider while it is dragged, so the card reads as bright or dim before anything is saved. Off, the
 * glow goes out. The switch sits top right where it is reached for; the brightness is saved when the slider is let go.
 */
export function LampTile({
  lamp,
  on,
  brightness,
  dimmable,
}: {
  lamp: Lamp;
  on: boolean;
  brightness: number;
  dimmable: boolean;
}) {
  const toggle = useLedSet(lamp);
  const dim = useLedSet(
    lamp === "network" ? "network_brightness" : "wifi_brightness",
  );
  // the value under the finger while dragging; null shows the device's own
  const [drag, setDrag] = useState<number | null>(null);
  const shown =
    toggle.isPending && toggle.variables != null
      ? toggle.variables === "1"
      : on;
  const level =
    drag ??
    (dim.isPending && dim.variables ? Number(dim.variables) : brightness);
  const { icon: Icon, color } = LAMP[lamp];
  const name = t(LAMP[lamp].label);
  // 0 when off; on, from a faint wash at 1 % to a strong glow at 100 % (non-dimmable lamps glow fully)
  const glow = shown ? (dimmable ? 0.12 + (level / 100) * 0.58 : 0.6) : 0;

  return (
    <section
      style={
        {
          "--lamp": color,
          "--glow": `${Math.round(glow * 100)}%`,
          "--halo": `${Math.round(glow * 40)}px`,
        } as CSSProperties
      }
      className="relative isolate flex flex-col gap-4 overflow-hidden rounded-2xl border bg-card p-5 transition-colors"
    >
      {/* the lamp's light: a wash from the icon's corner, its strength the brightness */}
      <div
        aria-hidden
        className="pointer-events-none absolute inset-0 -z-10 transition-[background] duration-300 ease-out"
        style={{
          background:
            "radial-gradient(130% 110% at 0% 0%, color-mix(in oklab, var(--lamp) var(--glow), transparent) 0%, transparent 70%)",
        }}
      />

      <div className="flex items-center gap-3">
        <span
          className={cn(
            "flex size-11 shrink-0 items-center justify-center rounded-full transition-[background-color,box-shadow,color] duration-300",
            shown ? "text-white" : "bg-muted text-muted-foreground",
          )}
          style={
            shown
              ? {
                  backgroundColor: "var(--lamp)",
                  boxShadow: "0 0 var(--halo) var(--lamp)",
                }
              : undefined
          }
        >
          <Icon className="size-5" strokeWidth={2.4} />
        </span>
        <div className="flex min-w-0 flex-1 flex-col">
          <h3 className="text-base leading-tight font-semibold">{name}</h3>
          <span className="text-sm text-muted-foreground tabular-nums">
            {shown
              ? dimmable
                ? t("system.leds.lamps.statusOn", { pct: level })
                : t("system.leds.lamps.on")
              : t("system.leds.lamps.off")}
          </span>
        </div>
        <AppleSwitch
          checked={shown}
          disabled={toggle.isPending}
          aria-busy={toggle.isPending}
          aria-label={name}
          onCheckedChange={(v) => toggle.mutate(v ? "1" : "0")}
        />
      </div>

      {dimmable && (
        <div
          className={cn(
            "flex items-center gap-3 transition-opacity",
            !shown && "opacity-45",
          )}
        >
          <SunDimIcon
            aria-hidden
            className="size-4 shrink-0 text-muted-foreground"
          />
          <Slider
            min={1}
            max={100}
            step={1}
            value={[level]}
            disabled={!shown || dim.isPending}
            aria-label={t("system.leds.lamps.brightnessLabel", { lamp: name })}
            onValueChange={([v]) => setDrag(v)}
            onValueCommit={([v]) => {
              setDrag(null);
              if (v !== brightness) dim.mutate(String(v));
            }}
            className="[&_[data-slot=slider-range]]:bg-(--lamp) [&_[data-slot=slider-thumb]]:size-5 [&_[data-slot=slider-thumb]]:border-0 [&_[data-slot=slider-thumb]]:shadow-md [&_[data-slot=slider-track]]:h-1.5 [&_[data-slot=slider-track]]:bg-foreground/10"
          />
          <SunIcon
            aria-hidden
            className="size-5 shrink-0 text-muted-foreground"
          />
        </div>
      )}

      <p className="text-sm leading-relaxed text-muted-foreground">
        {t(LAMP[lamp].description)}
        {lamp === "wifi" && t("system.leds.lamps.offNote")}
      </p>
    </section>
  );
}
