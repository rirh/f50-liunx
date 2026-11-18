import { type ReactNode, useRef } from "react";
import { Card } from "@/components/ui/card";
import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
} from "@/components/ui/collapsible";
import { prefersReducedMotion } from "@/hooks/use-motion";
import { t } from "@/i18n";
import { cn } from "@/lib/utils";
import { useGSAP } from "@gsap/react";
import { gsap } from "gsap";
import { ChevronRightIcon } from "lucide-react";

/**
 * A foldable card of rows (a subscription's nodes, the local nodes, a 策略组): the header is a title and one plain
 * line, its icon actions, then the disclosure chevron; folding leaves the header.
 *
 * The body stays mounted and GSAP folds it: the height eases in and out on Apple's curve, and on opening (and on
 * the first showing) the `[data-tile]` children rise in one after another. Closed, it is inert.
 */
export function NodeGroup({
  title,
  description,
  open,
  onOpenChange,
  actions,
  contentClassName,
  bare,
  children,
}: {
  title: ReactNode;
  description?: ReactNode;
  open: boolean;
  onOpenChange: (open: boolean) => void;
  actions?: ReactNode;
  contentClassName?: string;
  /** On a wide screen, no card: the header is a plain line on the page and the content sits right under it. */
  bare?: boolean;
  children: ReactNode;
}) {
  const body = useRef<HTMLDivElement>(null);
  // the last state drawn: a run with the same `open` (the first, or StrictMode's second) sets it without a fold
  const drawn = useRef<boolean | null>(null);
  useGSAP(
    () => {
      const el = body.current;
      if (!el) return;
      const first = drawn.current === null || drawn.current === open;
      drawn.current = open;
      const still = prefersReducedMotion();
      if (!open) {
        if (first || still) gsap.set(el, { height: 0, autoAlpha: 0 });
        else
          gsap.to(el, {
            height: 0,
            autoAlpha: 0,
            duration: 0.42,
            ease: "appleInOut",
            overwrite: true,
          });
        return;
      }
      if (still) {
        gsap.set(el, { clearProps: "height,opacity,visibility" });
        return;
      }
      if (!first)
        gsap.fromTo(
          el,
          { height: el.offsetHeight },
          {
            height: "auto",
            autoAlpha: 1,
            duration: 0.5,
            ease: "appleInOut",
            overwrite: true,
            clearProps: "height,opacity,visibility",
          },
        );
      gsap.from(el.querySelectorAll("[data-tile]"), {
        autoAlpha: 0,
        y: 8,
        scale: 0.985,
        duration: 0.6,
        ease: "apple",
        delay: first ? 0 : 0.06,
        stagger: { amount: 0.24 },
        clearProps: "opacity,visibility,transform",
      });
    },
    { dependencies: [open], scope: body },
  );
  return (
    <Collapsible open={open} onOpenChange={onOpenChange} asChild>
      <Card
        className={cn(
          "gap-0 py-0",
          bare &&
            "md:overflow-visible md:rounded-none md:bg-transparent md:ring-0",
        )}
      >
        <div
          className={cn(
            "flex min-h-14 items-center gap-1 pr-2",
            bare && "md:pr-0",
          )}
        >
          <CollapsibleTrigger
            className={cn(
              "flex min-w-0 flex-1 flex-col justify-center gap-0.5 self-stretch py-2.5 pl-4 text-left outline-none focus-visible:bg-muted/60",
              bare && "md:rounded-md md:pl-1",
            )}
          >
            <span
              className={cn(
                "truncate text-sm font-semibold",
                bare && "md:text-base",
              )}
            >
              {title}
            </span>
            {description && (
              <span className="truncate text-xs text-muted-foreground tabular-nums">
                {description}
              </span>
            )}
          </CollapsibleTrigger>
          {actions}
          <CollapsibleTrigger
            aria-label={open ? t("proxy.fold.close") : t("proxy.fold.open")}
            className="grid size-7 shrink-0 place-items-center rounded-md text-muted-foreground outline-none hover:bg-muted hover:text-foreground focus-visible:ring-3 focus-visible:ring-ring/50"
          >
            <ChevronRightIcon
              className={cn(
                "size-4 transition-transform duration-200 motion-reduce:transition-none",
                open && "rotate-90",
              )}
            />
          </CollapsibleTrigger>
        </div>
        <CollapsibleContent
          ref={body}
          forceMount
          inert={!open}
          className={cn(
            "overflow-hidden border-t",
            bare && "md:border-t-0",
            contentClassName,
          )}
        >
          {children}
        </CollapsibleContent>
      </Card>
    </Collapsible>
  );
}
