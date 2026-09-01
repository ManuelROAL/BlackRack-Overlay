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
  lib: read("src-tauri/src/lib.rs"),
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

// Every surface that has to enumerate the overlays repeats the roster. Probing
// for a substring only proves an overlay was added; reading each list back also
// catches the entries that outlive a rename or a removal.
const rustArrayEntries = (source, name, tuple) => {
  const declaration = new RegExp(
    `const ${name}: \\[(?:\\(&str, &str\\)|&str); (\\d+)\\] = \\[([\\s\\S]*?)\\];`
  ).exec(source);
  if (!declaration) {
    fail(`${name}: declaration was not found`);
    return [];
  }
  const entries = tuple
    ? [...declaration[2].matchAll(/\("([^"]+)", "([^"]+)"\)/g)].map((match) => match[1])
    : [...declaration[2].matchAll(/"([^"]+)"/g)].map((match) => match[1]);
  if (Number(declaration[1]) !== entries.length) {
    fail(`${name}: declared length ${declaration[1]} does not match ${entries.length} entries`);
  }
  return entries;
};

const viteInputKeys = () => {
  const source = ts.createSourceFile(
    "vite.config.ts",
    sources.vite,
    ts.ScriptTarget.Latest,
    true,
    ts.ScriptKind.TS
  );
  let keys = null;
  const visit = (node) => {
    if (ts.isPropertyAssignment(node)
      && ts.isIdentifier(node.name)
      && node.name.text === "input"
      && ts.isObjectLiteralExpression(node.initializer)) {
      keys = node.initializer.properties
        .filter((property) => ts.isPropertyAssignment(property) && ts.isIdentifier(property.name))
        .map((property) => property.name.text);
    }
    ts.forEachChild(node, visit);
  };
  visit(source);
  if (!keys) fail("vite: rollup input map was not found");
  return keys ?? [];
};

// The control panel, the composite host and the browser source index are shells,
// not overlays, so they are the only bundle entries allowed outside the roster.
const VITE_NON_OVERLAY_INPUTS = ["control", "browser", "composite"];

const compareRoster = (surface, actual, { ordered = false } = {}) => {
  const known = new Set(overlays);
  const seen = new Set(actual);
  const missing = overlays.filter((overlay) => !seen.has(overlay));
  const unknown = actual.filter((entry) => !known.has(entry));
  const duplicated = actual.filter((entry, index) => actual.indexOf(entry) !== index);
  if (missing.length > 0) fail(`${surface}: missing overlays: ${missing.join(", ")}`);
  if (unknown.length > 0) fail(`${surface}: unknown overlays: ${unknown.join(", ")}`);
  if (duplicated.length > 0) fail(`${surface}: duplicated overlays: ${duplicated.join(", ")}`);
  if (ordered && missing.length === 0 && unknown.length === 0 && duplicated.length === 0
    && actual.join(",") !== overlays.join(",")) {
    fail(`${surface}: order diverges from OverlayId: ${actual.join(", ")}`);
  }
};

const compositeFile = "src/composite.ts";
const compositeSource = ts.createSourceFile(
  compositeFile,
  read(compositeFile),
  ts.ScriptTarget.Latest,
  true,
  ts.ScriptKind.TS
);
const projections = new Map();
let compositeOverlayIds = null;
const visitComposite = (node) => {
  if (ts.isVariableDeclaration(node)
    && ts.isIdentifier(node.name)
    && node.name.text === "overlayIds"
    && node.initializer
    && ts.isArrayLiteralExpression(node.initializer)) {
    compositeOverlayIds = node.initializer.elements
      .filter(ts.isStringLiteral)
      .map((element) => element.text);
  }
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

if (compositeOverlayIds === null) {
  fail("composite: overlayIds was not found");
} else {
  // The composite host stacks the overlays in this order and the Rust layout seed
  // walks OVERLAY_LABELS in the same one, so both have to track OverlayId exactly.
  compareRoster("composite overlayIds", compositeOverlayIds, { ordered: true });
}
compareRoster("composite telemetryFields", [...projections.keys()]);
compareRoster("lib.rs OVERLAY_LABELS", rustArrayEntries(sources.lib, "OVERLAY_LABELS", false), {
  ordered: true
});
compareRoster(
  "browser_source.rs BROWSER_OVERLAYS",
  rustArrayEntries(sources.rust, "BROWSER_OVERLAYS", true)
);
compareRoster(
  "vite rollup input",
  viteInputKeys().filter((key) => !VITE_NON_OVERLAY_INPUTS.includes(key))
);

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
    `Validated ${overlays.length} overlay registrations across 8 surfaces, ${backendFrameFields.size} frame fields and telemetry projections.`
  );
}
