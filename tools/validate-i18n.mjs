import { readFileSync, readdirSync } from "node:fs";
import { extname, join } from "node:path";
import { fileURLToPath } from "node:url";
import ts from "typescript";

const projectRoot = fileURLToPath(new URL("../", import.meta.url));
const catalogPath = join(projectRoot, "src", "i18n", "catalogs.ts");
const indexPath = join(projectRoot, "src", "i18n", "index.ts");
const catalogSource = readFileSync(catalogPath, "utf8");
const catalogFile = ts.createSourceFile(catalogPath, catalogSource, ts.ScriptTarget.Latest, true);

const fail = (message) => { throw new Error(message); };
const unwrap = (node) => {
  while (ts.isAsExpression(node) || ts.isSatisfiesExpression(node) || ts.isParenthesizedExpression(node)) node = node.expression;
  return node;
};
const variableInitializer = (sourceFile, name) => {
  let result;
  sourceFile.forEachChild((node) => {
    if (!ts.isVariableStatement(node)) return;
    for (const declaration of node.declarationList.declarations) {
      if (ts.isIdentifier(declaration.name) && declaration.name.text === name && declaration.initializer) result = unwrap(declaration.initializer);
    }
  });
  return result;
};
const propertyName = (node) => {
  if (ts.isIdentifier(node) || ts.isStringLiteral(node) || ts.isNumericLiteral(node)) return node.text;
  fail(`Unsupported computed catalog property at ${node.getStart()}.`);
};
const stringValue = (node, context) => {
  const value = unwrap(node);
  if (ts.isStringLiteral(value) || ts.isNoSubstitutionTemplateLiteral(value)) return value.text;
  fail(`${context} must be a string literal.`);
};
const parseMessage = (node, context) => {
  const value = unwrap(node);
  if (ts.isStringLiteral(value) || ts.isNoSubstitutionTemplateLiteral(value)) return { kind: "text", text: value.text };
  if (!ts.isObjectLiteralExpression(value)) fail(`${context} is not a supported catalog message.`);
  const plural = {};
  for (const property of value.properties) {
    if (!ts.isPropertyAssignment(property)) fail(`${context} contains an unsupported plural entry.`);
    const category = propertyName(property.name);
    if (category !== "one" && category !== "other") fail(`${context} contains unsupported plural category ${category}.`);
    if (Object.hasOwn(plural, category)) fail(`${context} contains duplicate plural category ${category}.`);
    plural[category] = stringValue(property.initializer, `${context}.${category}`);
  }
  if (typeof plural.one !== "string" || typeof plural.other !== "string") fail(`${context} must define one and other plural forms.`);
  return { kind: "plural", ...plural };
};
const parseCatalogObject = (node, name, allowSpread) => {
  if (!ts.isObjectLiteralExpression(node)) fail(`${name} must be an object literal.`);
  const messages = new Map();
  let spreads = 0;
  for (const property of node.properties) {
    if (ts.isSpreadAssignment(property)) {
      spreads += 1;
      if (!allowSpread || !ts.isIdentifier(property.expression) || property.expression.text !== "es") fail(`${name} may only spread the Spanish catalog.`);
      continue;
    }
    if (!ts.isPropertyAssignment(property)) fail(`${name} contains an unsupported property.`);
    const key = propertyName(property.name);
    if (messages.has(key)) fail(`${name} contains duplicate key ${key}.`);
    messages.set(key, parseMessage(property.initializer, `${name}.${key}`));
  }
  if (allowSpread && spreads !== 1) fail(`${name} must spread the complete Spanish catalog exactly once.`);
  if (!allowSpread && spreads !== 0) fail(`${name} cannot contain spreads.`);
  return messages;
};

const es = parseCatalogObject(variableInitializer(catalogFile, "es"), "es", false);
const enOverrides = parseCatalogObject(variableInitializer(catalogFile, "en"), "en", true);
const en = new Map(es);
for (const [key, message] of enOverrides) {
  if (!es.has(key)) fail(`English catalog contains unknown key ${key}.`);
  en.set(key, message);
}
if (es.size !== en.size || [...es.keys()].some((key) => !en.has(key))) fail("Resolved Spanish and English catalogs do not contain equal keys.");
const inheritedAllowlist = JSON.parse(readFileSync(join(projectRoot, "tools", "i18n-inherited-message-allowlist.json"), "utf8"));
const allowedInheritedKeys = new Set(inheritedAllowlist.map(({ key }) => key));
const inheritedKeys = [...es.keys()].filter((key) => !enOverrides.has(key));
const unexpectedInherited = inheritedKeys.filter((key) => !allowedInheritedKeys.has(key));
const staleInherited = [...allowedInheritedKeys].filter((key) => !es.has(key) || enOverrides.has(key));
if (unexpectedInherited.length > 0) fail(`English catalog inherits unreviewed Spanish keys: ${unexpectedInherited.join(", ")}.`);
if (staleInherited.length > 0) fail(`English inherited-message allowlist contains stale keys: ${staleInherited.join(", ")}.`);

const parameters = (text) => new Set([...text.matchAll(/\{([A-Za-z][A-Za-z0-9]*)\}/g)].map((match) => match[1]));
const equalSets = (left, right) => left.size === right.size && [...left].every((value) => right.has(value));
for (const [key, spanish] of es) {
  const english = en.get(key);
  if (spanish.kind !== english.kind) fail(`Catalog message shape differs for ${key}.`);
  const variants = spanish.kind === "plural" ? ["one", "other"] : ["text"];
  for (const variant of variants) {
    const spanishParameters = parameters(spanish[variant]);
    const englishParameters = parameters(english[variant]);
    if (!equalSets(spanishParameters, englishParameters)) fail(`Catalog parameters differ for ${key}.${variant}: es=[${[...spanishParameters]}], en=[${[...englishParameters]}].`);
  }
}

const indexSource = readFileSync(indexPath, "utf8");
const supportedMatch = indexSource.match(/SUPPORTED_LOCALES\s*=\s*\[([^\]]+)\]/);
if (!supportedMatch) fail("Could not read SUPPORTED_LOCALES metadata.");
const supportedLocales = [...supportedMatch[1].matchAll(/["']([a-z][a-z0-9-]*)["']/g)].map((match) => match[1]);
const catalogMetadata = variableInitializer(catalogFile, "catalogs");
if (!ts.isObjectLiteralExpression(catalogMetadata)) fail("Could not read catalog locale metadata.");
const catalogLocales = catalogMetadata.properties.map((property) => {
  if (!ts.isShorthandPropertyAssignment(property)) fail("Catalog metadata must use locale shorthand properties.");
  return property.name.text;
});
if (!equalSets(new Set(supportedLocales), new Set(catalogLocales))) fail(`Supported locale metadata [${supportedLocales}] does not match catalog metadata [${catalogLocales}].`);

const htmlFiles = readdirSync(projectRoot).filter((file) => file.endsWith(".html"));
const attributeKeys = htmlFiles.flatMap((file) => {
  const source = readFileSync(join(projectRoot, file), "utf8");
  return [...source.matchAll(/data-i18n(?:-title|-aria-label|-placeholder)?="([^"]+)"/g)].map((match) => ({ file, key: match[1] }));
});
const unknownBindings = attributeKeys.filter(({ key }) => !es.has(key));
if (unknownBindings.length > 0) fail(`Unknown HTML localization keys: ${unknownBindings.map(({ file, key }) => `${file}: ${key}`).join(", ")}`);

const walk = (directory) => readdirSync(directory, { withFileTypes: true }).flatMap((entry) => {
  const path = join(directory, entry.name);
  return entry.isDirectory() ? walk(path) : [path];
});
const sourceFiles = walk(join(projectRoot, "src")).filter((file) => extname(file) === ".ts");
const literalCalls = [];
for (const file of sourceFiles) {
  const source = readFileSync(file, "utf8");
  const parsed = ts.createSourceFile(file, source, ts.ScriptTarget.Latest, true);
  const visit = (node) => {
    if (ts.isCallExpression(node) && ts.isIdentifier(node.expression) && node.expression.text === "t") {
      const argument = node.arguments[0];
      if (argument && ts.isStringLiteral(argument)) literalCalls.push({ file, key: argument.text });
    }
    ts.forEachChild(node, visit);
  };
  visit(parsed);
}
const unknownCalls = literalCalls.filter(({ key }) => !es.has(key));
if (unknownCalls.length > 0) fail(`Unknown TypeScript localization keys: ${unknownCalls.map(({ file, key }) => `${file}: ${key}`).join(", ")}`);

console.log(`i18n: ${es.size} equal catalog keys (${enOverrides.size} English overrides, ${inheritedKeys.length} reviewed neutral inheritances), compatible parameters, ${supportedLocales.length} locales, ${attributeKeys.length} HTML bindings and ${literalCalls.length} typed calls validated.`);
