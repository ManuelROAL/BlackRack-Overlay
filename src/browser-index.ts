import "./browser-index.css";
import { applyTranslations, getLocale } from "./i18n";

applyTranslations();
const locale = getLocale();
document.querySelectorAll<HTMLAnchorElement>("[data-browser-route]").forEach((link) => {
  const url = new URL(link.href);
  url.searchParams.set("lang", locale);
  link.href = `${url.pathname}${url.search}`;
});
