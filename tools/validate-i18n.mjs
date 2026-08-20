import { readFileSync } from "node:fs";

const catalogSource = readFileSync(new URL("../src/i18n/catalogs.ts", import.meta.url), "utf8");
const htmlSource = readFileSync(new URL("../index.html", import.meta.url), "utf8");
const esBlock = catalogSource.match(/export const es = \{([\s\S]*?)\n\} as const/)?.[1];
if (!esBlock) throw new Error("Could not read the Spanish localization catalog.");

const catalogKeys = [...esBlock.matchAll(/"([^"]+)":/g)].map((match) => match[1]);
const keySet = new Set(catalogKeys);
if (catalogKeys.length !== keySet.size) throw new Error("The Spanish localization catalog contains duplicate keys.");

const attributeKeys = [...htmlSource.matchAll(/data-i18n(?:-title|-aria-label|-placeholder)?="([^"]+)"/g)]
  .map((match) => match[1]);
const unknown = [...new Set(attributeKeys.filter((key) => !keySet.has(key)))];
if (unknown.length > 0) throw new Error(`Unknown localization keys in index.html: ${unknown.join(", ")}`);

if (!/export const en = \{[\s\S]*?\.\.\.es,[\s\S]*?\} as const satisfies Catalog;/.test(catalogSource)) {
  throw new Error("The English catalog must extend the complete Spanish key set and satisfy Catalog.");
}

console.log(`i18n: ${keySet.size} catalog keys and ${attributeKeys.length} control-panel bindings validated.`);
