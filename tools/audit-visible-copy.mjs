import { readFileSync, readdirSync } from "node:fs";
import { extname, join, relative } from "node:path";
import { fileURLToPath } from "node:url";
import ts from "typescript";

const root = fileURLToPath(new URL("../", import.meta.url));
const allowlistPath = join(root, "tools", "i18n-visible-copy-allowlist.json");
const allowlist = JSON.parse(readFileSync(allowlistPath, "utf8"));
const candidates = new Map();
const visibleProperties = new Set(["textContent", "innerText", "title", "ariaLabel", "placeholder"]);
const visibleAttributes = new Set(["title", "aria-label", "placeholder"]);
const humanText = (text) => {
  const normalized = text.replace(/\s+/g, " ").trim();
  if (!normalized || !/[A-Za-zÁÉÍÓÚÜÑáéíóúüñ]/.test(normalized)) return false;
  return /[a-záéíóúüñ]{3,}/.test(normalized);
};
const add = (file, kind, text) => {
  const normalized = text.replace(/\s+/g, " ").trim();
  if (!humanText(normalized)) return;
  const path = relative(root, file).replaceAll("\\", "/");
  const fingerprint = `${path}|${kind}|${normalized}`;
  candidates.set(fingerprint, { file: path, kind, text: normalized });
};
const literalTexts = (node) => {
  if (!node) return [];
  if (ts.isStringLiteral(node) || ts.isNoSubstitutionTemplateLiteral(node)) return [node.text];
  if (ts.isTemplateExpression(node)) return [node.head.text, ...node.templateSpans.map((span) => span.literal.text)];
  if (ts.isConditionalExpression(node)) return [...literalTexts(node.whenTrue), ...literalTexts(node.whenFalse)];
  if (ts.isBinaryExpression(node) && node.operatorToken.kind === ts.SyntaxKind.PlusToken) return [...literalTexts(node.left), ...literalTexts(node.right)];
  return [];
};
const property = (node) => ts.isPropertyAccessExpression(node) ? node.name.text : undefined;
const walk = (directory) => readdirSync(directory, { withFileTypes: true }).flatMap((entry) => {
  const path = join(directory, entry.name);
  return entry.isDirectory() ? walk(path) : [path];
});

for (const file of walk(join(root, "src")).filter((path) => extname(path) === ".ts" && !path.endsWith(join("i18n", "catalogs.ts")))) {
  const parsed = ts.createSourceFile(file, readFileSync(file, "utf8"), ts.ScriptTarget.Latest, true);
  const visit = (node) => {
    if (ts.isBinaryExpression(node) && node.operatorToken.kind === ts.SyntaxKind.EqualsToken && visibleProperties.has(property(node.left))) {
      for (const text of literalTexts(node.right)) add(file, property(node.left), text);
    }
    if (ts.isCallExpression(node)) {
      if (ts.isPropertyAccessExpression(node.expression) && node.expression.name.text === "setAttribute") {
        const attribute = node.arguments[0];
        if (ts.isStringLiteral(attribute) && visibleAttributes.has(attribute.text)) {
          for (const text of literalTexts(node.arguments[1])) add(file, attribute.text, text);
        }
      }
      if (ts.isIdentifier(node.expression) && node.expression.text === "node") {
        for (const text of literalTexts(node.arguments[2])) add(file, "node-content", text);
      }
    }
    if (ts.isNewExpression(node) && ts.isIdentifier(node.expression) && node.expression.text === "Option") {
      for (const text of literalTexts(node.arguments?.[0])) add(file, "option-label", text);
    }
    ts.forEachChild(node, visit);
  };
  visit(parsed);
}

for (const file of readdirSync(root).filter((name) => name.endsWith(".html")).map((name) => join(root, name))) {
  const source = readFileSync(file, "utf8");
  for (const match of source.matchAll(/<([a-z][a-z0-9-]*)([^>]*)>([^<>]+)<\/\1>/gi)) {
    if (/data-i18n(?:=|-)/.test(match[2])) continue;
    add(file, "html-text", match[3]);
  }
}

const allowed = new Set(allowlist.map(({ fingerprint }) => fingerprint));
const unexpected = [...candidates.keys()].filter((fingerprint) => !allowed.has(fingerprint));
const stale = [...allowed].filter((fingerprint) => !candidates.has(fingerprint));
if (unexpected.length > 0 || stale.length > 0) {
  if (unexpected.length > 0) console.error(`Unexpected visible-copy candidates:\n${unexpected.map((value) => `  ${value}`).join("\n")}`);
  if (stale.length > 0) console.error(`Stale visible-copy allowlist entries:\n${stale.map((value) => `  ${value}`).join("\n")}`);
  process.exitCode = 1;
} else {
  console.log(`i18n visible-copy audit: ${candidates.size} intentional literals matched the documented allowlist.`);
}
