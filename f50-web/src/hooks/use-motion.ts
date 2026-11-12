/**
 * GSAP for the live figures: a number that eases to its new value, and cards that come in one after another.
 *
 * Both honour `prefers-reduced-motion`: the value is set at once and the cards simply appear. Tweens belong to the
 * component (`useGSAP` reverts them on unmount), and nothing animates layout: only opacity, transforms and text.
 */
import { type RefObject, useRef } from "react";
import { useGSAP } from "@gsap/react";
import { gsap } from "gsap";

gsap.registerPlugin(useGSAP);

export function prefersReducedMotion() {
  return (
    typeof window !== "undefined" &&
    window.matchMedia?.("(prefers-reduced-motion: reduce)").matches
  );
}

/**
 * Eases the text of `ref` from the last value to `value` over `duration` seconds, writing `format(n)` on each
 * frame. `null` writes `format(null)` at once. Returns nothing: the element's text is the output.
 */
export function useTweenedText(
  ref: RefObject<HTMLElement | null>,
  value: number | null | undefined,
  format: (n: number | null) => string,
  duration = 0.6,
) {
  const shown = useRef<{ v: number | null }>({ v: null });
  useGSAP(
    () => {
      const el = ref.current;
      if (!el) return;
      const target = value ?? null;
      const from = shown.current.v;
      if (target == null || from == null || prefersReducedMotion()) {
        gsap.killTweensOf(shown.current);
        shown.current.v = target;
        el.textContent = format(target);
        return;
      }
      gsap.to(shown.current, {
        v: target,
        duration,
        ease: "power2.out",
        overwrite: true,
        onUpdate: () => {
          el.textContent = format(shown.current.v);
        },
      });
    },
    { dependencies: [value], scope: ref },
  );
}

/**
 * The direct children of `ref` that match `selector` rise in one after another, once: when `ready` first turns
 * true (the data arrived). Later updates never replay it.
 */
export function useStaggerIn(
  ref: RefObject<HTMLElement | null>,
  ready: boolean,
  selector = ":scope > *",
) {
  const done = useRef(false);
  useGSAP(
    () => {
      if (!ready || done.current || !ref.current) return;
      done.current = true;
      if (prefersReducedMotion()) return;
      gsap.from(ref.current.querySelectorAll(selector), {
        opacity: 0,
        y: 12,
        duration: 0.45,
        ease: "power2.out",
        stagger: 0.05,
        clearProps: "opacity,transform",
      });
    },
    { dependencies: [ready], scope: ref },
  );
}
