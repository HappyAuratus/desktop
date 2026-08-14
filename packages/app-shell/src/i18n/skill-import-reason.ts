import type { TFunction } from "i18next";
import { activeLocale, translationResources, type TranslationKey } from "./i18n-instance";

/** Looks up `key` in the active locale only, so missing English copy cannot fall back to Chinese. */
function translateKnown(t: TFunction, key: string, fallback: TranslationKey): string {
  const locale = activeLocale();
  if (key in translationResources[locale]) {
    return t(key as TranslationKey, { lng: locale });
  }
  return t(fallback, { lng: locale });
}

/** Localizes one stable skill-import error code into a user-facing sentence. */
export function localizeSkillImportReason(code: string | null | undefined, t: TFunction): string {
  if (code) {
    const importKey = `settings.skills.importReason.${code}`;
    if (importKey in translationResources[activeLocale()]) {
      return translateKnown(t, importKey, "settings.skills.importReason.unknown");
    }
    const errorKey = `errors.${code}`;
    if (errorKey in translationResources[activeLocale()]) {
      return translateKnown(t, errorKey, "settings.skills.importReason.unknown");
    }
  }
  return t("settings.skills.importReason.unknown", { lng: activeLocale() });
}

/** Localizes a candidate, result, or session status label. */
export function localizeSkillImportStatus(status: string, t: TFunction): string {
  return translateKnown(t, `settings.skills.importStatus.${status}`, "settings.skills.importStatus.unknown");
}

/** Explains why one committed candidate did not succeed, or `null` when it did. */
export function localizeSkillImportResultReason(
  result: { status: string; errorCode: string | null },
  t: TFunction,
): string | null {
  if (result.status === "failed") return localizeSkillImportReason(result.errorCode, t);
  if (result.status === "staleconflict") return t("settings.skills.importReason.stale_conflict", { lng: activeLocale() });
  return null;
}
