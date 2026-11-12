import { Button } from "@/components/ui/button";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuLabel,
  DropdownMenuRadioGroup,
  DropdownMenuRadioItem,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { LOCALES, type Locale, setLocale, t, useLocale } from "@/i18n";
import { LanguagesIcon } from "lucide-react";

/** The language menu in the header (and on the login page); the choice is remembered in this browser. */
export function LanguageSwitcher() {
  const locale = useLocale();
  return (
    <DropdownMenu>
      <DropdownMenuTrigger asChild>
        <Button variant="ghost" size="sm" aria-label={t("app.language")}>
          <LanguagesIcon data-icon="inline-start" />
          <span className="hidden sm:inline">
            {locale === "zh" ? "中文" : "EN"}
          </span>
        </Button>
      </DropdownMenuTrigger>
      <DropdownMenuContent align="end">
        <DropdownMenuLabel>{t("app.language")}</DropdownMenuLabel>
        <DropdownMenuRadioGroup
          value={locale}
          onValueChange={(v) => setLocale(v as Locale)}
        >
          {LOCALES.map((l) => (
            <DropdownMenuRadioItem
              key={l.id}
              value={l.id}
              lang={l.id === "zh" ? "zh-CN" : "en"}
            >
              {l.label}
            </DropdownMenuRadioItem>
          ))}
        </DropdownMenuRadioGroup>
      </DropdownMenuContent>
    </DropdownMenu>
  );
}
