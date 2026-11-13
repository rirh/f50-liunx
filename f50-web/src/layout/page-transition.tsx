import { type ReactNode, useLayoutEffect, useRef } from "react";
import { gsap } from "gsap";

/** The page's scroll area; a new route fades and rises in (one-template's DashboardPageTransition). */
export function PageTransition({
  children,
  routeKey,
}: {
  children: ReactNode;
  routeKey: string;
}) {
  const ref = useRef<HTMLDivElement>(null);
  useLayoutEffect(() => {
    const el = ref.current;
    if (!el) return;
    gsap.killTweensOf(el);
    if (window.matchMedia("(prefers-reduced-motion: reduce)").matches) {
      gsap.set(el, { autoAlpha: 1, clearProps: "transform" });
      return;
    }
    const ctx = gsap.context(() => {
      gsap.fromTo(
        el,
        { autoAlpha: 0.5, y: 7 },
        {
          autoAlpha: 1,
          y: 0,
          duration: 0.24,
          ease: "power2.out",
          clearProps: "transform,opacity,visibility",
        },
      );
    }, el);
    return () => ctx.revert();
  }, [routeKey]);
  return (
    <div
      ref={ref}
      className="flex min-h-0 flex-1 flex-col overflow-x-hidden overflow-y-auto"
    >
      {children}
    </div>
  );
}
