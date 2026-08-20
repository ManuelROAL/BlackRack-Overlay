import { applyTranslations, isLocale, LOCALE_STORAGE_KEY, setLocale } from "./index";

applyTranslations();

window.addEventListener("storage", (event) => {
  if (event.key !== LOCALE_STORAGE_KEY || !isLocale(event.newValue)) return;
  setLocale(event.newValue);
  window.location.reload();
});
