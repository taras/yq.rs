import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { readFile } from "node:fs/promises";
import { resolve } from "node:path";
import { pathToFileURL } from "node:url";

const [backend, workload] = process.argv.slice(2);
const installed = resolve("target/bench/install/node_modules/yq.rs/dist");
function collect() {
  if (typeof globalThis.gc === "function") {
    globalThis.gc();
  } else if (typeof globalThis.Bun?.gc === "function") {
    globalThis.Bun.gc(true);
  }
}
collect();
const baselineRss = process.memoryUsage().rss;
let engine;
if (backend !== "baseline") {
  const entry = await import(pathToFileURL(`${installed}/${backend === "native" ? "native.mjs" : "inline.js"}`));
  if (backend === "native") {
    assert.equal(entry.backend, "native");
  }
  engine = entry.yq;
}
await new Promise(resolve => setTimeout(resolve, 50));
collect();
const loadedRss = process.memoryUsage().rss;
const hash = createHash("sha256");
if (workload === "repeated") {
  const records = Array.from({ length: 16 }, (_, id) => ({ id, name: `user-${id}`, active: id % 3 === 0, score: (id * 37) % 100 }));
  const yaml = `items:\n${records.map(row => `  - id: ${row.id}\n    name: ${row.name}\n    active: ${row.active}\n    score: ${row.score}\n`).join("")}`;
  const cases = [
    [".name", "name: Ada\n"],
    [".items | map(select(.active == true)) | length", yaml],
    [".items | sort_by(.score) | .[0:3]", JSON.stringify({ items: records }), { inputFormat: "json", outputFormat: "json" }],
    [".items |= map(.score += 1)", yaml],
  ];
  for (const [expression, input, options] of cases) {
    let result;
    for (let i = 0; i < 10_000; i++) {
      result = engine.evaluate(expression, input, options);
    }
    hash.update(result);
  }
} else if (workload === "large") {
  const input = await readFile("target/bench/input-10000000.yml", "utf8");
  const output = engine.evaluate(".name", input);
  assert.equal(output, "Ada\n");
  hash.update(output);
}
const afterWorkRss = process.memoryUsage().rss;
collect();
console.log(JSON.stringify({
  runtime: process.versions.bun ? `Bun ${process.versions.bun}` : `Node ${process.versions.node}`,
  backend, workload, baselineRss, loadedRss, afterWorkRss,
  afterGcRss: process.memoryUsage().rss, digest: hash.digest("hex"),
}));
