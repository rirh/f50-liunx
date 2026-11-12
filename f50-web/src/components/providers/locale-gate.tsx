import { App } from "@/App";
import { useLocale } from "@/i18n";

/** Remounts the routed app when the language changes, so every `t()` reads the new one (the query cache stays). */
export function LocaleGate() {
  const locale = useLocale();
  return <App key={locale} />;
}
