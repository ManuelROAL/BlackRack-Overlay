import { readFileSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const directory = dirname(fileURLToPath(import.meta.url));
const inventoryPath = join(directory, "RaceOS-client-endpoints.md");
const collectionPath = join(directory, "RaceOS.complete.postman_collection.json");
const environmentPath = join(directory, "RaceOS.complete.postman_environment.json");
const inventory = readFileSync(inventoryPath, "utf8");

const sections = [];
let currentSection = null;
for (const line of inventory.split(/\r?\n/)) {
  const heading = line.match(/^## (.+)$/);
  if (heading) {
    currentSection = { name: heading[1], requests: [] };
    sections.push(currentSection);
    continue;
  }

  const endpoint = line.match(/^\| `(GET|POST|PUT|PATCH|DELETE|WS)` \| `([^`]+)`/);
  if (endpoint && currentSection) {
    currentSection.requests.push({ method: endpoint[1], path: endpoint[2] });
  }
}

const variablePath = (path) => path.replaceAll(/{([^}]+)}/g, "{{$1}}");
const isAuthentication = (path) => path === "/authenticate" || path === "/authenticate/steam";
const isMutation = (method, path) =>
  method !== "GET" && method !== "WS" && !isAuthentication(path);

const headersFor = (path, method) => {
  const headers = [];
  if (path.startsWith("/api/")) {
    headers.push({
      key: "Game-Authorization",
      value: "Bearer {{raceos_access_token}}",
      type: "text"
    });
  }
  if (method !== "GET" && method !== "WS") {
    headers.push({ key: "Content-Type", value: "application/json", type: "text" });
  }
  return headers;
};

const bodyFor = (method, path) => {
  if (method === "WS") return undefined;
  if (path === "/authenticate") {
    return {
      mode: "raw",
      raw: '{\n  "token": "{{auth_session_ticket}}",\n  "game": "lmu",\n  "platform": "steam"\n}',
      options: { raw: { language: "json" } }
    };
  }
  if (path === "/authenticate/steam") {
    return {
      mode: "raw",
      raw: "{{steam_openid_json}}",
      options: { raw: { language: "json" } }
    };
  }
  if (path === "/api/v1/players") {
    return {
      mode: "raw",
      raw: '{\n  "usernames": {{driver_usernames_json}}\n}',
      options: { raw: { language: "json" } }
    };
  }
  if (method === "GET" && path === "/api/v1/statistics/overall") {
    return {
      mode: "raw",
      raw: '{\n  "playerIds": {{player_ids_json}}\n}',
      options: { raw: { language: "json" } }
    };
  }
  if (method === "GET") return undefined;
  return {
    mode: "raw",
    raw: "{{request_body_json}}",
    options: { raw: { language: "json" } }
  };
};

const authTests = (path) => {
  if (path !== "/authenticate") return [];
  return [
    {
      listen: "test",
      script: {
        type: "text/javascript",
        exec: [
          "pm.test('RaceOS autentica correctamente', function () { pm.response.to.be.success; });",
          "const body = pm.response.json();",
          "pm.test('La respuesta contiene accessToken', function () {",
          "  pm.expect(body.accessToken).to.be.a('string').and.not.empty;",
          "});",
          "if (body.accessToken) pm.environment.set('raceos_access_token', body.accessToken);"
        ]
      }
    }
  ];
};

const requestItem = ({ method, path }) => {
  if (method === "WS") {
    return {
      name: `WS ${path} (referencia)`,
      request: {
        method: "GET",
        header: [],
        url: "{{raceos_websocket_url}}",
        description:
          "Referencia WebSocket. Créala como solicitud WebSocket en Postman y usa los subprotocolos access_token y el token actual; una colección HTTP v2.1 no puede representar correctamente esta conexión."
      }
    };
  }

  const mutation = isMutation(method, path);
  const body = bodyFor(method, path);
  return {
    name: `${method} ${path}`,
    event: authTests(path),
    request: {
      method,
      header: headersFor(path, method),
      ...(body ? { body } : {}),
      url: `{{raceos_base_url}}${variablePath(path)}`,
      description: mutation
        ? "MODIFICA DATOS. Bloqueada mientras allow_mutations no sea true. Sustituye request_body_json por el cuerpo que espera esta operación."
        : "Ruta extraída del cliente web distribuido con Le Mans Ultimate."
    }
  };
};

const localTicket = {
  name: "0. Obtener ticket local de LMU",
  event: [
    {
      listen: "test",
      script: {
        type: "text/javascript",
        exec: [
          "pm.test('La API local responde correctamente', function () { pm.response.to.be.success; });",
          "const body = pm.response.json();",
          "pm.test('La respuesta contiene authSessionTicket', function () {",
          "  pm.expect(body.authSessionTicket).to.be.a('string').and.not.empty;",
          "});",
          "if (body.authSessionTicket) pm.environment.set('auth_session_ticket', body.authSessionTicket);"
        ]
      }
    }
  ],
  request: {
    method: "GET",
    header: [],
    url: "{{lmu_local_base_url}}/rest/profile/getAuthSessionTicket",
    description:
      "Requiere Le Mans Ultimate abierto y Postman Desktop Agent. Guarda el ticket temporal en el entorno activo."
  }
};

const mutationGuard = {
  listen: "prerequest",
  script: {
    type: "text/javascript",
    exec: [
      "const method = pm.request.method.toUpperCase();",
      "const url = pm.request.url.toString();",
      "const isAuthentication = url.endsWith('/authenticate') || url.endsWith('/authenticate/steam');",
      "const mutatesData = method !== 'GET' && !isAuthentication;",
      "const allowed = String(pm.environment.get('allow_mutations')).toLowerCase() === 'true';",
      "if (mutatesData && !allowed) {",
      "  console.warn('Petición bloqueada. Establece allow_mutations=true conscientemente para ejecutarla.');",
      "  if (pm.execution && pm.execution.skipRequest) pm.execution.skipRequest();",
      "  else throw new Error('Petición bloqueada: allow_mutations=false');",
      "}"
    ]
  }
};

const collection = {
  info: {
    _postman_id: "12819455-48ed-4b61-b21e-fc5271472615",
    name: "LMU - Inventario completo RaceOS",
    description:
      "Colección generada desde las rutas referenciadas por el cliente web de Le Mans Ultimate. No es documentación oficial de RaceOS. Las mutaciones están bloqueadas por defecto.",
    schema: "https://schema.getpostman.com/json/collection/v2.1.0/collection.json"
  },
  event: [mutationGuard],
  item: [
    { name: "0. Ticket local de LMU", item: [localTicket] },
    ...sections
      .filter((section) => section.requests.length > 0)
      .map((section) => ({ name: section.name, item: section.requests.map(requestItem) }))
  ],
  protocolProfileBehavior: { disableBodyPruning: true }
};

const defaults = new Map([
  ["lmu_local_base_url", "http://127.0.0.1:6397"],
  ["raceos_base_url", "https://raceos.gg"],
  ["raceos_websocket_url", "wss://raceos.gg/ws"],
  ["auth_session_ticket", ""],
  ["raceos_access_token", ""],
  ["allow_mutations", "false"],
  ["driver_usernames_json", '["Manuel Rodriguez Alvarez"]'],
  ["player_ids_json", "[]"],
  ["request_body_json", "{}"],
  ["steam_openid_json", "{}"],
  ["eventType", "daily"],
  ["eventId", ""],
  ["page", "1"],
  ["take", "10"],
  ["boolean", "false"],
  ["status", "waiting"]
]);

const serializedCollection = JSON.stringify(collection);
for (const match of serializedCollection.matchAll(/{{([^}]+)}}/g)) {
  if (!defaults.has(match[1])) defaults.set(match[1], "");
}

const secretVariables = new Set(["auth_session_ticket", "raceos_access_token"]);
const environment = {
  id: "61f40bcb-0fb4-4af0-a704-18fdf26349ac",
  name: "LMU - RaceOS completo",
  values: [...defaults].map(([key, value]) => ({
    key,
    value,
    type: secretVariables.has(key) ? "secret" : "default",
    enabled: true
  })),
  _postman_variable_scope: "environment",
  _postman_exported_at: "2026-08-05T00:00:00.000Z",
  _postman_exported_using: "LMUOverlay generator"
};

writeFileSync(collectionPath, `${JSON.stringify(collection, null, 2)}\n`, "utf8");
writeFileSync(environmentPath, `${JSON.stringify(environment, null, 2)}\n`, "utf8");

console.log(`Colección: ${collectionPath}`);
console.log(`Entorno: ${environmentPath}`);
console.log(`Endpoints inventariados: ${sections.flatMap((section) => section.requests).length}`);
