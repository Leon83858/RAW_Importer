import type { Language } from "../types/generated/Language";
import type { Messages } from "./types";
import { de } from "./de";
import { en } from "./en";
import { fr } from "./fr";
import { nl } from "./nl";
import { nlBE } from "./nlBE";

export type { Messages } from "./types";

export const catalogues: Record<Language, Messages> = {
  english: en,
  german: de,
  french: fr,
  dutch: nl,
  dutch_belgium: nlBE,
};

/** BCP 47 tags, identical to `Language::tag()` in the Rust core. */
export const localeTag: Record<Language, string> = {
  english: "en-GB",
  german: "de-DE",
  french: "fr-FR",
  dutch: "nl-NL",
  dutch_belgium: "nl-BE",
};

/** Folder names produced by the core are always `MM EnglishMonth` (2.x compatible). */
export const layoutExample = {
  year_month_day: "2026/11 November/08/",
  year_month: "2026/11 November/",
  year: "2026/",
  day_month_year: "08-11-2026/",
} as const;
