# i18n

Two languages: 简体中文 (`zh`, the source) and English (`en`). No library: `src/i18n/index.ts` is the whole runtime.

## Using it

```ts
import { t, dateLocale, type Key } from "@/i18n";

t("dashboard.wifi.title"); // "Wi-Fi 热点" / "Wi-Fi hotspot"
t("dashboard.wifi.clients", { count: n }); // English picks `clients_one` when count === 1
t("common.unit.day", { n: 3 }); // {name} placeholders
format(date, "PP", { locale: dateLocale() }); // date-fns in the current language
```

- `t()` reads the current language. Switching language remounts the routed app (`LocaleGate`), so call `t()` **during
  render** (components, label helpers called from components, event handlers, toasts) — **never at module load**.
  A module-level `const LABEL = { a: "中文" }` must become keys (`const LABEL = { a: "ns.x.a" } satisfies
Record<A, Key>` then `t(LABEL[a])`) or a function (`const label = (a: A) => t(...)`).
- Zod schemas with messages: build them in a function (`const schema = () => z.object(...)`) or inside the component
  (`useMemo`), so the message is read in the current language.
- Text with markup: split it (`{t("a.before")}<code>x</code>{t("a.after")}`) or use a placeholder for a plain value.
- Keep in Chinese/English as it is: product and protocol names (Wi-Fi, SIM, APN, RSRP, AT, LTE, NR, DHCP, USB…),
  values the device returns, and code comments (comments stay as they are).

## Files

- `locales/zh/<namespace>.ts` — the source: `export default { ... }` (plain nested object of strings).
- `locales/en/<namespace>.ts` — `const m: Messages["<namespace>"] = { ... }; export default m;` Same keys, English
  text. A missing or extra key fails `tsc`.
- A key ending in `_one` is the English singular of the key without it (`clients` / `clients_one`); the Chinese file
  has both with the same text. Use `{count}` as the placeholder for such keys.
- Namespaces: `common` (shared words: actions, states, units), `app` (shell, login, shared components, errors,
  formatting), `nav`, `dashboard`, `cellular`, `sms`, `network`, `system` (overview, settings, CPU, LEDs),
  `maintenance` (updates, boot, logs), `tools` (AT terminal, jobs). Inside a namespace, nest by page then by component
  (`cellular.locks.bands.title`).

## English style

Short, plain, sentence case ("Save settings", not "Save Settings"). Router-UI vocabulary: hotspot, uplink, data usage,
billing cycle, band lock, cell lock, signal, sign in/out. Units keep their symbols (dBm, dB, MHz, GB, Mbps).
