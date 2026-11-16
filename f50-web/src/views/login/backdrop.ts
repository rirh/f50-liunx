import { useCallback, useEffect, useState } from "react";

/**
 * A random photo from Lorem Picsum (picsum.photos) behind the login page, with its author for the credit.
 *
 * The device is often reached without internet (a router being set up, no SIM yet), so nothing waits for it: the page
 * shows its own gradient, and the photo fades in only once it has loaded completely. A photo that does not load in
 * time, or at all, leaves the gradient.
 */
export interface Backdrop {
  src: string;
  author?: string;
  /** The photo's page on Unsplash. */
  link?: string;
}

interface PicsumPhoto {
  id: string;
  author: string;
  url: string;
}

const LIST_TIMEOUT_MS = 4000;
/** picsum's catalogue is about a thousand photos: ten pages of a hundred cover it. */
const PAGES = 10;

/** The viewport in device pixels, rounded up to hundreds (fewer distinct sizes, better cached), capped at 2560. */
function size() {
  const dpr = Math.min(window.devicePixelRatio || 1, 2);
  const round = (n: number) => Math.min(2560, Math.ceil((n * dpr) / 100) * 100);
  return { w: round(window.innerWidth), h: round(window.innerHeight) };
}

async function pick(): Promise<Backdrop> {
  const { w, h } = size();
  const ctl = new AbortController();
  const timer = setTimeout(() => ctl.abort(), LIST_TIMEOUT_MS);
  try {
    const page = 1 + Math.floor(Math.random() * PAGES);
    const r = await fetch(
      `https://picsum.photos/v2/list?page=${page}&limit=100`,
      { signal: ctl.signal },
    );
    const list = (await r.json()) as PicsumPhoto[];
    const p = list[Math.floor(Math.random() * list.length)];
    if (!p) throw new Error("empty list");
    return {
      src: `https://picsum.photos/id/${encodeURIComponent(p.id)}/${w}/${h}`,
      author: p.author,
      link: p.url,
    };
  } catch {
    // without the list: a random photo all the same, only without its credit
    return { src: `https://picsum.photos/${w}/${h}?random=${Date.now()}` };
  } finally {
    clearTimeout(timer);
  }
}

function preload(src: string): Promise<void> {
  return new Promise((resolve, reject) => {
    const img = new Image();
    img.decoding = "async";
    img.onload = () => resolve();
    img.onerror = () => reject(new Error("image failed"));
    img.src = src;
  });
}

/** The photo once it has loaded (null until then, and when it cannot), and `shuffle` for another. */
export function useBackdrop() {
  const [backdrop, setBackdrop] = useState<Backdrop | null>(null);
  const [round, setRound] = useState(0);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    let live = true;
    pick()
      .then(async (b) => {
        await preload(b.src);
        if (live) setBackdrop(b);
      })
      .catch(() => undefined)
      .finally(() => {
        if (live) setLoading(false);
      });
    return () => {
      live = false;
    };
  }, [round]);

  const shuffle = useCallback(() => {
    setLoading(true);
    setRound((n) => n + 1);
  }, []);
  return { backdrop, loading, shuffle };
}
