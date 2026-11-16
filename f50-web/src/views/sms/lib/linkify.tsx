import type { ReactNode } from "react";

/**
 * Web addresses in an SMS: with a scheme (`http://dx.10086.cn/TRvqEbu`), or bare with a common top-level domain
 * (`dx.10086.cn/TRvqEbu`, as operators and banks write them). An address is ASCII: it ends at the first CJK character
 * (`dx.10086.cn/TRvqEbu查询` links `dx.10086.cn/TRvqEbu`).
 */
const PATH = String.raw`[A-Za-z0-9\-._~:/?#\[\]@!$&*+,;=%]`;
const URL_RE = new RegExp(
  String.raw`https?://[A-Za-z0-9.-]+(?::\d{2,5})?(?:/${PATH}*)?` +
    "|" +
    String.raw`(?<![@\w.-])(?:[a-z0-9](?:[a-z0-9-]{0,61}[a-z0-9])?\.)+(?:cn|com|net|org|top|vip|xyz|info|cc|co|me|io|tv|app|link|site|shop|ltd)(?![\w-])(?::\d{2,5})?(?:/${PATH}*)?`,
  "gi",
);

/** Punctuation that ends a sentence rather than an address. */
const TRAILING = /[.,;:!?)\]}'"]+$/u;

/** One SMS's text with its addresses as links that open in a new tab (no referrer: the console's address stays). */
export function linkify(text: string): ReactNode[] {
  const out: ReactNode[] = [];
  let last = 0;
  for (const m of text.matchAll(URL_RE)) {
    const start = m.index;
    const raw = m[0].replace(TRAILING, "");
    if (!raw) continue;
    if (start > last) out.push(text.slice(last, start));
    const href = /^https?:\/\//i.test(raw) ? raw : `http://${raw}`;
    out.push(
      <a
        key={start}
        href={href}
        target="_blank"
        rel="noopener noreferrer nofollow"
        className="break-all underline decoration-current/40 underline-offset-2 hover:decoration-current"
        onClick={(e) => e.stopPropagation()}
      >
        {raw}
      </a>,
    );
    last = start + raw.length;
  }
  if (last < text.length) out.push(text.slice(last));
  return out;
}
