import i18next from "i18next";
import tr from "./tr.json";

i18next.init({
  lng: "tr",
  fallbackLng: "tr",
  resources: { tr: { translation: tr } },
  interpolation: { escapeValue: false },
});

/** True when the text exists, so a screen can use a specific wording and fall back to the general one. */
export const has = (key: string): boolean => i18next.exists(key);

export const t = (key: string, opts?: Record<string, unknown>): string =>
  i18next.t(key, opts) as string;
