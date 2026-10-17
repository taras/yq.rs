import assert from "node:assert/strict";
import { readFile, rename } from "node:fs/promises";
import { fileURLToPath } from "node:url";

const wasmPath = fileURLToPath(import.meta.resolve("yq.rs/yq.wasm"));
const bytes = await readFile(wasmPath);
const hiddenPath = `${wasmPath}.hidden`;
let inline;
await rename(wasmPath, hiddenPath);
try {
  inline = await import("yq.rs");
  const alias = await import("yq.rs/inline");
  assert.equal(alias.yq, inline.yq);
} finally {
  await rename(hiddenPath, wasmPath);
}

const loader = await import("yq.rs/loader");
assert.deepEqual(WebAssembly.Module.imports(await WebAssembly.compile(bytes)), []);
assert.equal(inline.YqError, loader.YqError);

const engines = [
  ["inline ready instance", inline.yq],
  ["inline factory", await inline.createYq()],
  ["raw bytes loader", await loader.createYq(bytes)],
  ["compiled module loader", await loader.createYq(await WebAssembly.compile(bytes))],
];
const cases = [
  [".a.b[0]", "a:\n  b: [1, 2]\n", "1\n"],
  [".a.b |= map(. * 2)", "a:\n  b: [1, 2] # c\n", "a:\n  b: [2, 4] # c\n"],
  ['.x as $v | {"v": $v}', "x: 1\n", "v: 1\n"],
  [".a", 'a: "x" # c\n', '"x" # c\n', { unwrapScalar: false }],
  [".a", 'a: "x" # c\n', "x\n"],
  [".a", 'a: "x"\n', '"x"\n', { outputFormat: "json" }],
  [".a", 'a: {b: [1, "2"]}\n', '{"b":[1,"2"]}\n', { outputFormat: "json", indent: 0 }],
  [".", '{"a": 1}', "a: 1\n", { inputFormat: "json" }],
  [".", "a:\n  b: 1\n", "a:\n    b: 1\n", { indent: 4 }],
  ['"new"', "ignored: [invalid", "new\n", { nullInput: true }],
  ['strenv(NAME)', "", "München 😀\n", { nullInput: true, env: { NAME: "München 😀" } }],
  ['strenv(NAME)', "", 'left } right { "quoted" \\ \b\f\n', { nullInput: true, env: { NAME: 'left } right { "quoted" \\ \b\f' } }],
  ["now", "", "2026-10-17T12:34:56Z\n", { nullInput: true, now: "2026-10-17T12:34:56Z" }],
  ['load("settings.yml").enabled', "", "true\n", { nullInput: true, files: { "settings.yml": "enabled: true\n" } }],
  ['load("settings.yml").name', "", "{text}\n", { nullInput: true, files: { "settings.yml": 'name: "{text}"\n' } }],
];

for (const [name, engine] of engines) {
  for (const [expression, input, expected, options] of cases) {
    assert.equal(engine.evaluate(expression, input, options), expected, `${name}: ${expression}`);
  }
  assert.throws(() => engine.evaluate("[", ""), inline.YqError);
  assert.throws(() => engine.evaluate(".", "a: ["), inline.YqError);
  assert.throws(() => engine.evaluate(".", "", { unsupportedOption: true }), /unknown option: unsupportedOption/);
  const large = "é😀".repeat(40_000);
  assert.equal(engine.evaluate(".", JSON.stringify(large), { inputFormat: "json" }), `${large}\n`);
  assert.equal(engine.evaluate(".ok", "ok: true\n"), "true\n");
  console.log(`ok ${name}: ${cases.length} cases, errors, Unicode, memory growth and reuse`);
}

console.log(`Packaged ESM smoke passed on ${process.versions.bun ? `Bun ${process.versions.bun}` : `Node ${process.versions.node}`}`);
