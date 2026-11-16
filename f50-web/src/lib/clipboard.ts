/**
 * Text to the clipboard. The console is usually opened over plain http (`http://192.168.x.x`), which is not a secure
 * context: `navigator.clipboard` is missing there, so a hidden textarea and `execCommand("copy")` stand in (still
 * supported by every browser for a user gesture). Rejects when neither worked.
 */
export async function copyText(text: string): Promise<void> {
  if (window.isSecureContext && navigator.clipboard) {
    try {
      await navigator.clipboard.writeText(text);
      return;
    } catch {
      // a denied permission: the fallback may still work
    }
  }
  const area = document.createElement("textarea");
  area.value = text;
  area.setAttribute("readonly", "");
  area.style.position = "fixed";
  area.style.top = "0";
  area.style.left = "0";
  area.style.opacity = "0";
  area.style.pointerEvents = "none";
  // a dialog with a focus trap would take the focus back: the area goes into the focused element's dialog if any
  const host =
    document.activeElement?.closest("[role=dialog]") ?? document.body;
  host.appendChild(area);
  area.select();
  area.setSelectionRange(0, text.length);
  let ok = false;
  try {
    ok = document.execCommand("copy");
  } finally {
    area.remove();
  }
  if (!ok) throw new Error("copy failed");
}
