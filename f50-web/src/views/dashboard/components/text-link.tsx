import type { ReactNode } from "react";
import { Link } from "react-router-dom";
import { cn } from "@/lib/utils";
import { Slot } from "radix-ui";

// The project has no shared UnderlineHover yet: this is the web-build skill's component, kept local to the page.
const UNDERLINE_HOVER_CSS = `
  .underline-hover { position: relative; display: inline-block; width: fit-content; max-width: 100%; }
  .underline-hover::after {
    position: absolute; right: 0; bottom: 0; left: 0; height: 2px; pointer-events: none; content: "";
    background: linear-gradient(to right, var(--muted), var(--primary)) no-repeat;
    background-position: right bottom; background-size: 0 2px; transition: background-size 0.5s;
  }
  .underline-hover:hover::after, .underline-hover:focus-visible::after {
    background-position: left bottom; background-size: 100% 1px;
  }
  @media (prefers-reduced-motion: reduce) { .underline-hover::after { transition: none; } }
`;

export function UnderlineHover({
  asChild = false,
  className,
  ...props
}: React.ComponentProps<"span"> & { asChild?: boolean }) {
  const Comp = asChild ? Slot.Root : "span";
  return (
    <>
      <style href="underline-hover" precedence="default">
        {UNDERLINE_HOVER_CSS}
      </style>
      <Comp
        data-slot="underline-hover"
        className={cn("underline-hover", className)}
        {...props}
      />
    </>
  );
}

/** A text link to another page (never for buttons). */
export function TextLink({
  to,
  children,
  className,
}: {
  to: string;
  children: ReactNode;
  className?: string;
}) {
  return (
    <UnderlineHover asChild className={className}>
      <Link to={to}>{children}</Link>
    </UnderlineHover>
  );
}
