import i18next from "i18next";
import tr from "./tr.json";

i18next.init({
  lng: "tr",
  fallbackLng: "tr",
  resources: { tr: { translation: tr } },
  interpolation: { escapeValue: false },
});

export const t = (key: string, opts?: Record<string, unknown>): string =>
  i18next.t(key, opts) as string;
