import fs from "node:fs";
import path from "node:path";
import ts from "typescript";

const root = process.cwd();
const read = (file) => fs.readFileSync(path.join(root, file), "utf8");
const fail = (message) => {
  console.error(`Overlay validation failed: ${message}`);
  process.exitCode = 1;
};

const appearanceSource = ts.createSourceFile(
  "src/overlay-appearance.ts",
  read("src/overlay-appearance.ts"),
  ts.ScriptTarget.Latest,
  true,
  ts.ScriptKind.TS
);
let overlays = [];
appearanceSource.forEachChild((node) => {
  if (!ts.isTypeAliasDeclaration(node) || node.name.text !== "OverlayId") return;
  if (!ts.isUnionTypeNode(node.type)) return;
  overlays = node.type.types
    .filter(ts.isLiteralTypeNode)
    .map((type) => type.literal)
    .filter(ts.isStringLiteral)
    .map((literal) => literal.text);
});
if (overlays.length === 0) fail("OverlayId has no string members");

const sources = {
  control: read("index.html"),
  browser: read("browser.html"),
  vite: read("vite.config.ts"),
  rust: read("src-tauri/src/browser_source.rs"),
  docs: read("docs/overlays/README.md")
};
const patterns = {
  control: (overlay) => `data-overlay="${overlay}"`,
  browserUrls: (overlay) => `data-browser-overlay="${overlay}"`,
  browser: (overlay) => `href="/${overlay}"`,
  vite: (overlay) => `${overlay}: "${overlay}.html"`,
  rust: (overlay) => `("${overlay}", "${overlay}.html")`,
  docs: (overlay) => `| \`/${overlay}\` |`
};

for (const overlay of overlays) {
  for (const [surface, pattern] of Object.entries(patterns)) {
    const source = surface === "browserUrls" ? sources.control : sources[surface];
    const expected = pattern(overlay);
    const matches = source.split(expected).length - 1;
    if (matches !== 1) fail(`${overlay}: expected one ${surface} registration, found ${matches}`);
  }
  if (!fs.existsSync(path.join(root, `${overlay}.html`))) fail(`${overlay}: missing HTML entry`);
  if (!fs.existsSync(path.join(root, `src/${overlay}.ts`))) fail(`${overlay}: missing renderer`);
  if (!fs.existsSync(path.join(root, `docs/overlays/${overlay}.md`))) fail(`${overlay}: missing owning document`);
}

const compositeFile = "src/composite.ts";
const compositeSource = ts.createSourceFile(
  compositeFile,
  read(compositeFile),
  ts.ScriptTarget.Latest,
  true,
  ts.ScriptKind.TS
);
const projections = new Map();
const visitComposite = (node) => {
  if (ts.isVariableDeclaration(node)
    && ts.isIdentifier(node.name)
    && node.name.text === "telemetryFields"
    && node.initializer
    && ts.isObjectLiteralExpression(node.initializer)) {
    for (const property of node.initializer.properties) {
      if (!ts.isPropertyAssignment(property) || !ts.isIdentifier(property.name)
        || !ts.isArrayLiteralExpression(property.initializer)) continue;
      projections.set(property.name.text, new Set(
        property.initializer.elements.filter(ts.isStringLiteral).map((element) => element.text)
      ));
    }
  }
  ts.forEachChild(node, visitComposite);
};
visitComposite(compositeSource);

const telemetryTypesFile = "src/telemetry-types.ts";
const telemetryTypesSource = ts.createSourceFile(
  telemetryTypesFile,
  read(telemetryTypesFile),
  ts.ScriptTarget.Latest,
  true,
  ts.ScriptKind.TS
);
const frontendFrameFields = new Set();
telemetryTypesSource.forEachChild((node) => {
  if (!ts.isInterfaceDeclaration(node) || node.name.text !== "TelemetryFrame") return;
  for (const member of node.members) {
    if (ts.isPropertySignature(member) && member.name && ts.isIdentifier(member.name)) {
      frontendFrameFields.add(member.name.text);
    }
  }
});

const telemetryRust = read("src-tauri/src/telemetry/mod.rs");
const rustFrameBody = telemetryRust.match(/pub struct TelemetryFrame \{([\s\S]*?)\r?\n\}/)?.[1];
const backendFrameFields = new Set();
if (!rustFrameBody) {
  fail("Rust TelemetryFrame was not found");
} else {
  let skipNextField = false;
  for (const line of rustFrameBody.split(/\r?\n/)) {
    if (line.includes("#[serde(skip)]")) {
      skipNextField = true;
      continue;
    }
    const field = line.match(/^\s*([A-Za-z_][A-Za-z0-9_]*):/)?.[1];
    if (!field) continue;
    if (!skipNextField) backendFrameFields.add(field);
    skipNextField = false;
  }
}
const frontendOnlyFields = [...frontendFrameFields].filter((field) => !backendFrameFields.has(field));
const backendOnlyFields = [...backendFrameFields].filter((field) => !frontendFrameFields.has(field));
if (frontendOnlyFields.length > 0) {
  fail(`TelemetryFrame fields only in TypeScript: ${frontendOnlyFields.join(", ")}`);
}
if (backendOnlyFields.length > 0) {
  fail(`TelemetryFrame fields only in Rust: ${backendOnlyFields.join(", ")}`);
}

const telemetryFieldsUsedBy = (overlay) => {
  const file = `src/${overlay}.ts`;
  const source = ts.createSourceFile(file, read(file), ts.ScriptTarget.Latest, true, ts.ScriptKind.TS);
  const frameNames = new Set();
  const typeContainsFrame = (type) => type?.getText(source).includes("TelemetryFrame") ?? false;
  const collectNames = (node) => {
    if (ts.isParameter(node) && ts.isIdentifier(node.name) && typeContainsFrame(node.type)) {
      frameNames.add(node.name.text);
    }
    if (ts.isVariableDeclaration(node) && ts.isIdentifier(node.name)) {
      if (typeContainsFrame(node.type)
        || (node.initializer && ts.isAsExpression(node.initializer)
          && node.initializer.type.getText(source) === "TelemetryFrame")) {
        frameNames.add(node.name.text);
      }
    }
    if (ts.isCallExpression(node)
      && ts.isIdentifier(node.expression)
      && node.expression.text === "listenTelemetry") {
      const listener = node.arguments[0];
      if (listener && (ts.isArrowFunction(listener) || ts.isFunctionExpression(listener))) {
        for (const parameter of listener.parameters) {
          if (ts.isIdentifier(parameter.name)) frameNames.add(parameter.name.text);
        }
      }
    }
    ts.forEachChild(node, collectNames);
  };
  collectNames(source);

  const fields = new Set();
  const collectFields = (node) => {
    if (ts.isPropertyAccessExpression(node)
      && ts.isIdentifier(node.expression)
      && frameNames.has(node.expression.text)) {
      fields.add(node.name.text);
    }
    if (ts.isElementAccessExpression(node)
      && ts.isIdentifier(node.expression)
      && frameNames.has(node.expression.text)
      && node.argumentExpression
      && ts.isStringLiteral(node.argumentExpression)) {
      fields.add(node.argumentExpression.text);
    }
    ts.forEachChild(node, collectFields);
  };
  collectFields(source);
  return fields;
};

for (const overlay of overlays) {
  const projected = projections.get(overlay);
  if (!projected) {
    fail(`${overlay}: missing composite telemetry projection`);
    continue;
  }
  const used = telemetryFieldsUsedBy(overlay);
  const missing = [...used].filter((field) => !projected.has(field));
  const unused = [...projected].filter((field) => !used.has(field));
  if (missing.length > 0) fail(`${overlay}: unprojected telemetry fields: ${missing.join(", ")}`);
  if (unused.length > 0) fail(`${overlay}: unused projected telemetry fields: ${unused.join(", ")}`);
}

if (!process.exitCode) {
  console.log(
    `Validated ${overlays.length} overlay registrations, ${backendFrameFields.size} frame fields and telemetry projections.`
  );
}
