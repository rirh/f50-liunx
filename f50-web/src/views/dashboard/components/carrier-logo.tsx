import { cn } from "@/lib/utils";
import { RadioTowerIcon } from "lucide-react";
import type { Carrier } from "../carrier";

/** public/carriers (see its SOURCES.md): the carriers' own emblems. cbn.png is the full lockup; its left square is the mark. */
const SRC: Record<Carrier, string> = {
  cmcc: "carriers/cmcc.svg",
  cucc: "carriers/cucc.svg",
  ctcc: "carriers/ctcc.svg",
  cbn: "carriers/cbn.png",
};

/** The carrier's emblem on a white tile (as an app icon sits); a neutral tower for any other network. */
export function CarrierLogo({
  carrier,
  className,
}: {
  carrier: Carrier | null;
  className?: string;
}) {
  const tile = cn(
    "flex size-11 shrink-0 items-center justify-center overflow-hidden rounded-[12px] bg-white shadow-[0_0_0_0.5px_rgb(0_0_0/0.12),0_1px_2px_rgb(0_0_0/0.06)]",
    className,
  );
  if (!carrier)
    return (
      <span className={cn(tile, "bg-muted text-muted-foreground")} aria-hidden>
        <RadioTowerIcon className="size-5" />
      </span>
    );
  return (
    <span className={tile} aria-hidden>
      <img
        src={`${import.meta.env.BASE_URL}${SRC[carrier]}`}
        alt=""
        draggable={false}
        className={cn(
          "size-[34px]",
          carrier === "cbn" ? "object-cover object-left" : "object-contain",
        )}
      />
    </span>
  );
}
