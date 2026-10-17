import assert from "node:assert/strict";
import { readFile, writeFile } from "node:fs/promises";
import { cpus, platform, arch, release } from "node:os";
import { createHash } from "node:crypto";
import { resolve } from "node:path";
import { pathToFileURL } from "node:url";

const installed = resolve(process.argv[2] ?? "target/bench/install/node_modules/yq.rs/dist");
const wasm = await import(pathToFileURL(`${installed}/inline.js`));
const native = await import(pathToFileURL(`${installed}/native.mjs`));
assert.equal(native.backend, "native", "Benchmark requires an installed native addon");
const records = Array.from({ length: 16 }, (_, id) => ({ id, name: `user-${id}`, active: id % 3 === 0, score: (id * 37) % 100 }));
const yaml = `items:\n${records.map(row => `  - id: ${row.id}\n    name: ${row.name}\n    active: ${row.active}\n    score: ${row.score}\n`).join("")}`;
const cases = [
  ["YAML lookup", ".name", "name: Ada\n"],
  ["YAML filtering (16 records)", ".items | map(select(.active == true)) | length", yaml],
  ["JSON sorting (16 records)", ".items | sort_by(.score) | .[0:3]", JSON.stringify({ items: records }), { inputFormat: "json", outputFormat: "json" }],
  ["YAML update (16 records)", ".items |= map(.score += 1)", yaml],
];
const runs = 5;
const iterations = 10_000;
const results = {};
for (const [name, expression, input, options] of cases) {
  const expected = wasm.yq.evaluate(expression, input, options);
  assert.equal(native.yq.evaluate(expression, input, options), expected);
  const engines = { wasm: wasm.yq, native: native.yq };
  const samples = { wasm: [], native: [] };
  for (const engine of Object.values(engines)) {
    for (let i = 0; i < 500; i++) {
      engine.evaluate(expression, input, options);
    }
  }
  for (let batch = 0; batch < runs; batch++) {
    for (const backend of batch % 2 === 0 ? ["wasm", "native"] : ["native", "wasm"]) {
      const start = performance.now();
      let bytes = 0;
      for (let i = 0; i < iterations; i++) {
        bytes += engines[backend].evaluate(expression, input, options).length;
      }
      samples[backend].push(performance.now() - start);
      assert.equal(bytes, expected.length * iterations);
    }
  }
  const median = values => [...values].sort((a, b) => a - b)[Math.floor(values.length / 2)];
  results[name] = { wasm_ms: median(samples.wasm), native_ms: median(samples.native), samples };
  console.log(name, results[name]);
}
await writeFile("target/bench/library-results.json", JSON.stringify({
  date: new Date().toISOString(), node: process.versions.node, machine: `${platform()} ${arch()} ${release()}`, cpu: cpus()[0].model,
  iterations, runs, warmup: 500, results,
  sources: Object.fromEntries(await Promise.all(["crates/yq-node/src/lib.rs", "js/native.mjs"].map(async path => [path, createHash("sha256").update(await readFile(path)).digest("hex")]))),
}, null, 2) + "\n");
