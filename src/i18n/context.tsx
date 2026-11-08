import { createContext, useContext, type ReactNode } from "react";
import type { Language } from "../types/generated/Language";
import { catalogues, localeTag, type Messages } from "./catalogues";

type I18n = { t: Messages; language: Language; locale: string };

const I18nContext = createContext<I18n>({ t: catalogues.english, language: "english", locale: "en-GB" });

export function I18nProvider({ language, children }: { language: Language; children: ReactNode }) {
  const value = { t: catalogues[language], language, locale: localeTag[language] };
  return <I18nContext.Provider value={value}>{children}</I18nContext.Provider>;
}

// eslint-disable-next-line react-refresh/only-export-components
export function useI18n(): I18n {
  return useContext(I18nContext);
}
