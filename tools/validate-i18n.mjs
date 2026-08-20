import { readFileSync, readdirSync } from "node:fs";
import { fileURLToPath } from "node:url";

const catalogSource = readFileSync(new URL("../src/i18n/catalogs.ts", import.meta.url), "utf8");
const projectRoot = fileURLToPath(new URL("../", import.meta.url));
const htmlFiles = readdirSync(projectRoot).filter((file) => file.endsWith(".html"));
const esBlock = catalogSource.match(/export const es = \{([\s\S]*?)\n\} as const/)?.[1];
if (!esBlock) throw new Error("Could not read the Spanish localization catalog.");

const catalogKeys = [...esBlock.matchAll(/"([^"]+)":/g)].map((match) => match[1]);
const keySet = new Set(catalogKeys);
if (catalogKeys.length !== keySet.size) throw new Error("The Spanish localization catalog contains duplicate keys.");

const attributeKeys = htmlFiles.flatMap((file) => {
  const source = readFileSync(new URL(`../${file}`, import.meta.url), "utf8");
  return [...source.matchAll(/data-i18n(?:-title|-aria-label|-placeholder)?="([^"]+)"/g)]
    .map((match) => ({ file, key: match[1] }));
});
const unknown = attributeKeys.filter(({ key }) => !keySet.has(key));
if (unknown.length > 0) {
  throw new Error(`Unknown localization keys: ${unknown.map(({ file, key }) => `${file}: ${key}`).join(", ")}`);
}

if (!/export const en = \{[\s\S]*?\.\.\.es,[\s\S]*?\} as const satisfies Catalog;/.test(catalogSource)) {
  throw new Error("The English catalog must extend the complete Spanish key set and satisfy Catalog.");
}

console.log(`i18n: ${keySet.size} catalog keys and ${attributeKeys.length} HTML bindings validated across ${htmlFiles.length} documents.`);
