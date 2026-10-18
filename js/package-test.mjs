import assert from "node:assert/strict";
import { execFileSync, spawnSync } from "node:child_process";
import { chmod, copyFile, mkdtemp, readFile, rename, rm, stat, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { platformTarget } from "./platform.mjs";
import { fileURLToPath } from "node:url";

function npm(args, options) {
  if (process.platform === "win32") {
    return execFileSync(process.execPath, [join(dirname(process.execPath), "node_modules/npm/bin/npm-cli.js"), ...args], options);
  }
  return execFileSync("npm", args, options);
}

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
  let tarball = process.env.YQ_PACKAGE_TARBALL;
  if (!tarball) {
    const packed = JSON.parse(npm(["pack", "--ignore-scripts", "--json", "--cache", cache, "--pack-destination", fixture], { cwd: root, encoding: "utf8" }))[0];
    tarball = join(fixture, packed.filename);
  }
  tarball = resolve(tarball);
  const packed = JSON.parse(execFileSync(process.platform === "win32" ? "python" : "python3", [join(root, ".github/release/verify.py"), "inspect-package", tarball], { encoding: "utf8" }));
  const paths = packed.files.map(file => file.path);
  for (const required of ["dist/cli.mjs", "dist/cli-wasm.mjs", "dist/platform.mjs", "dist/inline.js", "dist/inline.d.ts", "dist/yq.js", "dist/yq.d.ts", "dist/yq.wasm", "LICENSE", "THIRD_PARTY_NOTICES.md"]) {
    assert.ok(paths.includes(required), `Missing package file: ${required}`);
  }
  assert.ok(paths.some(path => path.startsWith("crates/yq-core/licenses/")), "Missing third-party license texts");
  assert.ok(!paths.some(path => path.startsWith("js/")), "Package must ship compiled JavaScript");
  await writeFile(join(fixture, "package.json"), '{"private":true,"type":"module"}\n');
  npm(["install", "--prefix", fixture, "--cache", cache, "--ignore-scripts", "--no-package-lock", "--no-audit", "--no-fund", "--offline", tarball], {
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
  assert.equal(metadata.optionalDependencies, undefined);
  assert.equal(metadata.exports["./native"], undefined);
  const platforms = JSON.parse(await readFile(new URL("../.github/release/platforms.json", import.meta.url), "utf8"));
  for (const platform of platforms) {
    const path = `dist/native/${platform.id}/${platform.os === "win32" ? "yq.exe" : "yq"}`;
    const member = packed.files.find(file => file.path === path);
    if (process.env.YQ_PACKAGE_TARBALL) {
      assert.ok(member, `Missing bundled native binary: ${path}`);
    }
    if (member) {
      assert.equal(member.mode & 0o777, 0o755, `Lost executable mode: ${path}`);
    }
  }
  const target = platformTarget();
  const relative = `dist/native/${target}/${process.platform === "win32" ? "yq.exe" : "yq"}`;
  const hasNative = Boolean(target && paths.includes(relative));
  const native = join(fixture, "node_modules/yq.rs", relative);
  // Optional-dependency omission must not change native availability.
  npm(["install", "--prefix", fixture, "--cache", cache, "--ignore-scripts", "--no-package-lock", "--no-audit", "--no-fund", "--offline", "--omit=optional", tarball], { cwd: fixture, stdio: "inherit" });
  for (const runtime of runtimes) {
    const command = runtime === "node" ? process.execPath : runtime;
    const bin = join(fixture, "node_modules/yq.rs/dist/cli.mjs");
    assert.equal(execFileSync(command, [bin, ".name"], { cwd: fixture, input: "name: Ada\n", encoding: "utf8" }), "Ada\n");
    assert.equal(execFileSync(command, [bin, "-ne", '\"false\"'], { cwd: fixture, encoding: "utf8" }), "false\n");
    assert.match(execFileSync(command, [bin, "--version"], { cwd: fixture, encoding: "utf8" }), /Go yq v4\.54\.1/);
    assert.equal(execFileSync(command, [bin, ".name"], { cwd: fixture, input: "name: Ada\n", encoding: "utf8", env: { ...process.env, YQ_FORCE_WASM: "1" } }), "Ada\n");
    if (hasNative) {
      const paths = ["inline.js", "cli-wasm.mjs"].map(name => join(fixture, "node_modules/yq.rs/dist", name));
      for (const path of paths) {
        await rename(path, `${path}.hidden`);
      }
      try {
        assert.equal(execFileSync(command, [bin, ".name"], { cwd: fixture, input: "name: Ada\n", encoding: "utf8" }), "Ada\n", "Native dispatch must run without loading wasm");
      } finally {
        for (const path of paths) {
          await rename(`${path}.hidden`, path);
        }
      }
    }
    if (hasNative && process.platform !== "win32") {
      const bytes = await readFile(native);
      const mode = (await stat(native)).mode & 0o777;
      const assertFallback = code => {
        const result = spawnSync(command, [bin, ".name"], { cwd: fixture, input: "name: Ada\n", encoding: "utf8" });
        assert.equal(result.status, 0, `${code} must fall back to wasm`);
        assert.equal(result.stdout, "Ada\n");
        assert.equal(result.stderr, "");
      };
      try {
        await rename(native, `${native}.hidden`);
        try {
          assertFallback("missing binary");
        } finally {
          await rename(`${native}.hidden`, native);
        }
        await chmod(native, 0o644);
        assertFallback("EACCES");
        await chmod(native, 0o755);
        await writeFile(native, "#!/yq-test-missing-interpreter\n");
        assertFallback("ENOENT");
        await writeFile(native, "#!/bin/sh\nprintf 'native partial\\n'\nexit 42\n");
        const failure = spawnSync(command, [bin, ".name"], { cwd: fixture, input: "name: Ada\n", encoding: "utf8" });
        assert.equal(failure.status, 42, "An executed binary's failure must not retry through wasm");
        assert.equal(failure.stdout, "native partial\n");
        assert.equal(failure.stderr, "");
      } finally {
        await writeFile(native, bytes);
        await chmod(native, mode);
      }
    }
    execFileSync(command, [join(fixture, "smoke.mjs")], {
      cwd: fixture,
      stdio: "inherit",
    });
    execFileSync(command, [join(fixture, "smoke.mjs")], {
      cwd: fixture,
      stdio: "inherit",
      env: { ...process.env, YQ_FORCE_WASM: "1" },
    });

  }
} finally {
  await rm(fixture, { recursive: true, force: true });
}
