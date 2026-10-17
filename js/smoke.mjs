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

const native = await import("yq.rs/native");
assert.equal(native.YqError, inline.YqError);
if (process.env.YQ_TEST_NATIVE_ADDON_PACKAGE && !process.versions.bun && process.env.YQ_FORCE_WASM !== "1") {
  assert.equal(native.backend, "native", "Addon test must exercise the native backend");
} else {
  assert.equal(native.backend, "wasm");
  assert.equal(native.yq, inline.yq);
}
const engines = [
  ["native entry ready instance", native.yq],
  ["native entry factory", await native.createYq()],
  ["inline ready instance", inline.yq],
  ["inline factory", await inline.createYq()],
  ["raw bytes loader", await loader.createYq(bytes)],
  ["compiled module loader", await loader.createYq(await WebAssembly.compile(bytes))],
];
const cases = [
  [".name", "\uFEFFname: Ada\n", "Ada\n"],
  [".name", 'name: "\uFEFFAda"\n', "\uFEFFAda\n"],
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
  assert.deepEqual(engine.evaluateWithStatus("false", "", { nullInput: true }), { output: "false\n", matched: false });
  assert.deepEqual(engine.evaluateWithStatus('"false"', "", { nullInput: true }), { output: "false\n", matched: true });
  assert.equal(engine.evaluate(".a", "a: one\n---\na: two\n", { printDocSeparators: false }), "one\ntwo\n");
  assert.deepEqual(engine.evaluateSequenceWithStatus(".a", ["a: one\n", "---\na: two\n"]), { output: "one\n---\ntwo\n", matched: true });
  assert.throws(() => engine.evaluateSequenceWithStatus(".a", ["a: one\n", "[invalid"]), error => error instanceof inline.YqError && error.output === "one\n");
  assert.throws(() => engine.evaluate("[", ""), inline.YqError);
  assert.throws(() => engine.evaluate(".", "a: ["), inline.YqError);
  assert.throws(() => engine.evaluate(".", "", { unsupportedOption: true }), /unknown option: unsupportedOption/);
  const large = "é😀".repeat(40_000);
  assert.equal(engine.evaluate(".", JSON.stringify(large), { inputFormat: "json" }), `${large}\n`);
  assert.equal(engine.evaluate(".ok", "ok: true\n"), "true\n");
  console.log(`ok ${name}: ${cases.length} cases, errors, Unicode, memory growth and reuse`);
}

console.log(`Packaged ESM smoke passed on ${process.versions.bun ? `Bun ${process.versions.bun}` : `Node ${process.versions.node}`}`);
