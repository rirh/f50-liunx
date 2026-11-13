import { useEffect, useLayoutEffect, useRef } from "react";
import { t } from "@/i18n";
import type { NavItem } from "@/layout/nav";
import { cn } from "@/lib/utils";
import { ChevronLeftIcon, ChevronRightIcon, XIcon } from "lucide-react";

/** The pages opened in this visit as chrome-like tabs (one-template's tags view); the last one cannot be closed. */
export function RouteTags({
  active,
  items,
  onClose,
  onSelect,
}: {
  active: NavItem | null;
  items: readonly NavItem[];
  onClose: (item: NavItem) => void;
  onSelect: (item: NavItem) => void;
}) {
  const rootRef = useRef<HTMLDivElement>(null);
  const scrollRef = useRef<HTMLDivElement>(null);
  const firstActive = items[0]?.path === active?.path;

  useEffect(() => {
    scrollRef.current
      ?.querySelector<HTMLElement>('[data-active-tag="true"]')
      ?.scrollIntoView({ block: "nearest", inline: "center" });
  }, [active?.path, items]);

  useLayoutEffect(() => {
    const root = rootRef.current;
    const scroll = scrollRef.current;
    if (!root || !scroll) return;
    let frame = 0;
    const update = () => {
      window.cancelAnimationFrame(frame);
      frame = window.requestAnimationFrame(() => {
        root.dataset.overflow =
          scroll.scrollWidth > scroll.clientWidth + 1 ? "true" : "false";
      });
    };
    update();
    const observer =
      typeof ResizeObserver === "undefined" ? null : new ResizeObserver(update);
    observer?.observe(scroll);
    window.addEventListener("resize", update);
    return () => {
      window.cancelAnimationFrame(frame);
      window.removeEventListener("resize", update);
      observer?.disconnect();
    };
  }, [items]);

  const scrollBy = (left: number) =>
    scrollRef.current?.scrollBy({ left, behavior: "smooth" });

  return (
    <div ref={rootRef} className="tags-view-chrome min-w-0 shrink-0">
      <button
        type="button"
        aria-label={t("app.shell.scrollLeft")}
        className="tags-view-chrome__nav tags-view-chrome__nav--left"
        onClick={() => scrollBy(-240)}
      >
        <ChevronLeftIcon />
      </button>
      <div ref={scrollRef} className="tags-view-chrome__wrapper">
        <div
          className={cn(
            "tags-view-chrome__list",
            firstActive && "is-first-active",
          )}
        >
          {items.map((item, index) => {
            const Icon = item.icon;
            const on = active?.path === item.path;
            const closable = items.length > 1;
            const label = t(item.title);
            const separated =
              index > 0 && !on && items[index - 1]?.path !== active?.path;
            return (
              <div
                key={item.path}
                data-active-tag={on}
                className={cn(
                  "tags-view-chrome__item",
                  on && "active",
                  closable && "is-closable",
                  separated && "is-separated",
                )}
              >
                <button
                  type="button"
                  aria-current={on ? "page" : undefined}
                  className="tags-view-chrome__label"
                  onClick={() => onSelect(item)}
                >
                  <span className="tags-view-chrome__icon">
                    <Icon aria-hidden />
                  </span>
                  <span>{label}</span>
                </button>
                {closable && (
                  <button
                    type="button"
                    tabIndex={-1}
                    aria-label={t("app.shell.closeTab", { name: label })}
                    className="tags-view-chrome__close"
                    onClick={() => onClose(item)}
                  >
                    <XIcon aria-hidden />
                  </button>
                )}
              </div>
            );
          })}
        </div>
      </div>
      <button
        type="button"
        aria-label={t("app.shell.scrollRight")}
        className="tags-view-chrome__nav tags-view-chrome__nav--right"
        onClick={() => scrollBy(240)}
      >
        <ChevronRightIcon />
      </button>
    </div>
  );
}
