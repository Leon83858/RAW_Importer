import type { Messages } from "./types";
import { nl } from "./nl";

// Dutch as used in Belgium (Flemish). Belgian software conventionally
// addresses the user with the formal "u"; everything else follows nl-NL.
export const nlBE: Messages = {
  ...nl,
  stepTitles: { ...nl.stepTitles, setup: "Wat wilt u importeren?" },
  checkBeforeStart: "Controleer de doelmappen voordat u begint.",
  updateCheck: "Controleren op updates",
  updateAvailable: (version) => `Versie ${version} is beschikbaar.`,
  updateCurrent: (version) => `U gebruikt de nieuwste versie (${version}).`,
  updateLocalAhead: (current, latest) => `Deze build (${current}) is nieuwer dan de nieuwste release (${latest}).`,
  updateFailed: "Updates controleren is niet gelukt. Controleer uw verbinding en probeer het opnieuw.",
  updateAutoCheck: "Automatisch controleren bij opstarten",
  updatePromptTitle: "Automatische updatecontroles toestaan?",
  updatePromptBody: "RAW Importer kan GitHub controleren op een nieuwere stabiele release. Alleen de openbare releasepagina wordt opgevraagd; er wordt nooit automatisch een update gedownload of geïnstalleerd.",
  updatePromptSave: "Keuze opslaan",
  updatePromptNotNow: "Niet nu",
  settingsTitle: "Instellingen",
  settingsClose: "Sluiten",
  settingsLanguageLabel: "Taal van de interface",
  creditsLink: "@thelomphotography op Instagram",
  creditsOpenFailed: "De creditslink kon niet worden geopend.",
};
