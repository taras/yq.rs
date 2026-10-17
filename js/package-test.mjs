import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { copyFile, mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";

const root = fileURLToPath(new URL("../", import.meta.url));
const requested = process.argv.slice(2);
const runtimes = requested.length === 0 ? ["node", "bun"] : requested;
for (const runtime of runtimes) {
  if (runtime !== "node" && runtime !== "bun") {
    throw new Error(`Unsupported smoke runtime: ${runtime}`);
  }
}

const fixture = await mkdtemp(join(tmpdir(), "yq-npm-smoke-"));
try {
  const cache = join(fixture, ".npm-cache");
  const packed = JSON.parse(execFileSync("npm", ["pack", "--ignore-scripts", "--json", "--cache", cache, "--pack-destination", fixture], {
    cwd: root,
    encoding: "utf8",
  }))[0];
  const paths = packed.files.map(file => file.path);
  for (const required of ["dist/cli.mjs", "dist/inline.js", "dist/inline.d.ts", "dist/yq.js", "dist/yq.d.ts", "dist/yq.wasm", "LICENSE", "THIRD_PARTY_NOTICES.md"]) {
    assert.ok(paths.includes(required), `Missing package file: ${required}`);
  }
  assert.ok(paths.some(path => path.startsWith("crates/yq-core/licenses/")), "Missing third-party license texts");
  assert.ok(!paths.some(path => path.startsWith("js/")), "Package must ship compiled JavaScript");
  await writeFile(join(fixture, "package.json"), '{"private":true,"type":"module"}\n');
  execFileSync("npm", ["install", "--prefix", fixture, "--cache", cache, "--ignore-scripts", "--no-package-lock", "--no-audit", "--no-fund", "--offline", join(fixture, packed.filename)], {
    stdio: "inherit",
    cwd: fixture,
  });
  await copyFile(new URL("./smoke.mjs", import.meta.url), join(fixture, "smoke.mjs"));
  await writeFile(join(fixture, "consumer.mts"), `import { createYq, yq, YqError, type EvalOptions, type Evaluation, type Yq } from "yq.rs";
import { createYq as createInline } from "yq.rs/inline";
import { createYq as loadBytes } from "yq.rs/loader";
const options: EvalOptions = { inputFormat: "yaml", outputFormat: "json", indent: 2, nullInput: false, unwrapScalar: false, env: { A: "b" }, now: "2026-10-17T00:00:00Z", files: { "a.yml": "a: b" } };
const instance: Yq = await createYq();
yq.evaluate(".", "a: b", options);
instance.evaluate(".", "a: b");
const result: Evaluation = instance.evaluateSequenceWithStatus(".a", ["a: b", "a: c"]);
result.output.toUpperCase();
result.matched.valueOf();
await createInline();
await loadBytes(new Uint8Array());
new YqError("message");
// @ts-expect-error Only YAML and JSON are supported.
yq.evaluate(".", "", { inputFormat: "xml" });
// @ts-expect-error The inline entry needs no wasm argument.
await createYq(new Uint8Array());
`);
  execFileSync(process.execPath, [join(root, "node_modules/typescript/bin/tsc"), "--noEmit", "--strict", "--target", "ES2022", "--module", "NodeNext", "--moduleResolution", "NodeNext", join(fixture, "consumer.mts")], {
    cwd: fixture,
    stdio: "inherit",
  });
  const metadata = JSON.parse(await readFile(join(fixture, "node_modules/yq.rs/package.json"), "utf8"));
  assert.equal(metadata.bin.yq, "dist/cli.mjs");
  for (const runtime of runtimes) {
    const command = runtime === "node" ? process.execPath : runtime;
    const bin = join(fixture, "node_modules/.bin/yq");
    assert.equal(execFileSync(command, [bin, ".name"], { cwd: fixture, input: "name: Ada\n", encoding: "utf8" }), "Ada\n");
    assert.equal(execFileSync(command, [bin, "-ne", '\"false\"'], { cwd: fixture, encoding: "utf8" }), "false\n");
    assert.match(execFileSync(command, [bin, "--version"], { cwd: fixture, encoding: "utf8" }), /Go yq v4\.54\.1/);
    execFileSync(command, [join(fixture, "smoke.mjs")], {
      cwd: fixture,
      stdio: "inherit",
    });
  }
} finally {
  await rm(fixture, { recursive: true, force: true });
}
