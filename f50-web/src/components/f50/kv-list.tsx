import type { ReactNode } from "react";

export interface KvItem {
  label: string;
  value: ReactNode;
  /** A title on the value (the exact time behind "3 分钟前"). */
  title?: string;
}

/** Facts as label/value rows: dense, aligned, wrapping on small screens. */
export function KvList({ items }: { items: KvItem[] }) {
  return (
    <dl className="grid grid-cols-[minmax(7rem,auto)_1fr] gap-x-4 gap-y-2 text-sm">
      {items.map((it) => (
        <div key={it.label} className="contents">
          <dt className="text-muted-foreground">{it.label}</dt>
          <dd className="min-w-0 break-words" title={it.title}>
            {it.value ?? "—"}
          </dd>
        </div>
      ))}
    </dl>
  );
}
