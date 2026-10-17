import assert from "node:assert/strict";
import { spawn, spawnSync } from "node:child_process";
import { chmod, mkdir, mkdtemp, readFile, rm, stat, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { isAbsolute, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = fileURLToPath(new URL("../", import.meta.url));
const requested = process.argv.slice(2);
const runtimes = requested.length === 0 ? ["node", "bun"] : requested;
for (const runtime of runtimes) {
  assert.ok(runtime === "node" || runtime === "bun", `Unsupported CLI test runtime: ${runtime}`);
}
const reference = resolve(root, process.env.YQ_REFERENCE ?? "ref/yq-ref");
const environment = { ...process.env, NO_COLOR: "1", TERM: "dumb", YQ_CLI_TEST_NUMBER: "42", YQ_CLI_TEST_TEXT: "hello world", YQ_CLI_TEST_SPECIAL: 'quoted "braces {and}"\b\f' };
delete environment.FORCE_COLOR;
const fixture = await mkdtemp(join(tmpdir(), "yq-cli-"));
let checks = 0;

function run(command, args, input = "", cwd = fixture) {
  const result = spawnSync(command, args, { input, cwd, env: environment, encoding: "utf8", timeout: 30_000 });
  if (result.error) {
    throw new Error(`Could not execute ${command}: ${result.error.message}`, { cause: result.error });
  }
  assert.equal(result.signal, null, `${command} terminated with ${result.signal}`);
  return { status: result.status, stdout: result.stdout, stderr: result.stderr };
}

function runWithClosedStdout(command, args) {
  return new Promise((resolve, reject) => {
    const child = spawn(command, args, {
      cwd: fixture,
      env: environment,
      stdio: ["ignore", "pipe", "pipe"],
      timeout: 30_000,
    });
    let stderr = "";
    child.once("error", reject);
    child.stderr.setEncoding("utf8");
    child.stderr.on("data", chunk => {
      stderr += chunk;
    });
    child.once("close", (status, signal) => {
      resolve({ status: signal === "SIGPIPE" ? 141 : status, stderr });
    });
    child.stdout.destroy();
  });
}

try {
  assert.equal(run(reference, ["--version"]).status, 0, `YQ_REFERENCE must point to the pinned Go yq executable: ${reference}`);
  const cache = join(fixture, ".npm-cache");
  const packed = JSON.parse(run("npm", ["pack", "--ignore-scripts", "--json", "--cache", cache, "--pack-destination", fixture], "", root).stdout)[0];
  assert.ok(packed.files.some(file => file.path === "dist/cli.mjs"), "npm package must include the compiled CLI");
  await writeFile(join(fixture, "package.json"), '{"private":true,"type":"module"}\n');
  const installed = run("npm", ["install", "--prefix", fixture, "--cache", cache, "--ignore-scripts", "--no-package-lock", "--no-audit", "--no-fund", "--offline", join(fixture, packed.filename)]);
  assert.equal(installed.status, 0, installed.stderr);
  const packageRoot = join(fixture, "node_modules/yq.rs");
  const metadata = JSON.parse(await readFile(join(packageRoot, "package.json"), "utf8"));
  const bin = typeof metadata.bin === "string" ? metadata.bin : metadata.bin?.yq;
  assert.equal(typeof bin, "string", "npm package must expose a yq executable");
  const declaredEntry = isAbsolute(bin) ? bin : join(packageRoot, bin);
  const entry = join(fixture, "node_modules/.bin/yq");
  assert.equal((await stat(declaredEntry)).mode & 0o111, 0o111, "npm CLI must be executable");
  const version = metadata.version;
  await mkdir(join(fixture, "data dir"));
  const files = {
    "data dir/input.yml": "name: Ada\ncount: 2\nitems:\n  - name: beta\n    score: 2\n  - name: alpha\n    score: 1\n",
    "data.json": '{"name":"Ada","count":2,"items":[2,1]}\n',
    "bom.yml": "\uFEFFname: Ada\n",
    "bom.json": '\uFEFF{"name":"Ada"}\n',
    "second.yml": "name: Grace\ncount: 3\n",
    "documents.yml": "name: Ada\n---\nname: Grace\n",
    "query.yq": ".name\n",
    "message.txt": "first line\nsecond line\n",
    "config.properties": "name=Ada\ncount=2\n",
    "encoded.txt": "aGVsbG8=\n",
    "special.txt": 'quoted "braces {and}"\b\f\n',
    "help": "name: Help\n",
    "leading.yml": "---\nname: Ada\n",
    "comment.yml": "# hello\n---\n# about Ada\nname: Ada\n",
    "tail.yml": "name: Grace\n# last\n",
    "scalar.yml": '"---"\n',
    "scalar-multiline.yml": '"first\\n---\\nlast"\n',
    "bad.yml": "a: [\n",
  };
  for (const [path, content] of Object.entries(files)) {
    await writeFile(join(fixture, path), content);
  }
  await writeFile(join(fixture, "large.yml"), `value: ${"x".repeat(512 * 1024)}\n`);
  const input = files["data dir/input.yml"];
  const cases = [
    ["stdin lookup", [".name"], input],
    ["YAML BOM stdin lookup", [".name"], files["bom.yml"]],
    ["YAML BOM identity", ["."], files["bom.yml"]],
    ["YAML BOM file lookup", [".name", "bom.yml"]],
    ["YAML BOM with CRLF", [".name"], "\uFEFFname: Ada\r\n"],
    ["YAML BOM document marker", ["."], "\uFEFF---\nname: Ada\n"],
    ["YAML BOM leading comment", ["."], "\uFEFF# heading\nname: Ada\n"],
    ["YAML BOM version directive", [".name"], "\uFEFF%YAML 1.1\n---\nname: Ada\n"],
    ["YAML BOM after leading content", [".name"], "# heading\n\uFEFFname: Ada\n"],
    ["YAML BOM flow mapping", [".name"], '\uFEFF{"name":"Ada"}\n'],
    ["YAML BOM inside scalar", [".name"], 'name: "\uFEFFAda"\n'],
    ["YAML BOM in later key", ['.["\uFEFFname"]'], "first: 1\n\uFEFFname: Ada\n"],
    ["BOM from_yaml", ["-n", '"\uFEFFname: Ada\\n" | from_yaml | .name']],
    ["BOM literal YAML load", ["-n", 'load("bom.yml").name']],
    ["BOM string load preserves content", ["-n", 'load_str("bom.yml")']],
    ["identity default", [], input],
    ["eval alias", ["eval", ".count"], input],
    ["e alias and stdin dash", ["e", ".count", "-"], input],
    ["explicit expression", ["--expression", ".name", "data dir/input.yml"]],
    ["forced expression with file named help", ["--expression", ".name", "help"]],
    ["file with spaces", [".name", "data dir/input.yml"]],
    ["JSON filename auto detection", [".", "data.json"]],
    ["JSON filename scalar default", [".name", "data.json"]],
    ["JSON stdin explicit", ["-p", "json", "-o", "json", ".name"], files["data.json"]],
    ["JSON input alias", ["--input-format", "json", "--output-format", "yaml", ".items"], files["data.json"]],
    ["JSON output", ["-o", "json", "."], input],
    ["compact JSON", ["-o=json", "-I=0", "."], input],
    ["four-space YAML indent", ["--indent", "4", "."], input],
    ["raw scalar explicit", ["-r", ".name"], input],
    ["quoted scalar", ["-r=false", ".name"], input],
    ["JSON raw scalar", ["-o=json", "-r", ".name"], input],
    ["unwrap scalar long", ["--unwrapScalar=false", ".name"], input],
    ["null input", ["-n", '{"name": "Ada", "count": 2}']],
    ["null input long", ["--null-input", "[1, 2, 3] | length"]],
    ["environment typed", ["-n", "env(YQ_CLI_TEST_NUMBER)"]],
    ["null input rejects files", ["-n", ".", "data dir/input.yml"]],
    ["null input rejects implicit file", ["-n", "data dir/input.yml"]],
    ["environment controls and quotes", ["-n", "-o=json", "strenv(YQ_CLI_TEST_SPECIAL)"]],
    ["environment string", ["-n", "strenv(YQ_CLI_TEST_TEXT)"]],
    ["literal YAML load", ["-n", 'load("data dir/input.yml") | .name']],
    ["load text inside string", ["-n", JSON.stringify('load("missing-cli-test.yml")')]],
    ["load text inside comment", ["-n", '42 # load("missing-cli-test.yml")']],
    ["dynamic load text inside string", ["-n", JSON.stringify("load(.path)")]],
    ["dynamic load text inside comment", ["-n", "42 # load(.path)"]],
    ["unreachable missing literal load", ["-n", 'select(false) | load("missing-cli-test.yml")']],
    ["loaded file controls and quotes", ["-n", "-o=json", 'load_str("special.txt")']],
    ["nonexistent literal load", ["-n", 'load("missing-cli-test.yml")']],
    ["literal JSON load", ["-n", 'load("data.json") | .count']],
    ["literal string load", ["-n", 'load_str("message.txt")']],
    ["literal properties load", ["-n", 'load_props("config.properties")']],
    ["literal base64 load", ["-n", 'load_base64("encoded.txt")']],
    ["expression from file", ["--from-file", "query.yq", "data dir/input.yml"]],
    ["multiple documents", [".name", "documents.yml"]],
    ["no document separators", ["-N", ".name", "documents.yml"]],
    ["document separator inside raw scalar", ["-n", "-N", JSON.stringify("first\n---\nsecond")]],
    ["no document separators long", ["--no-doc", ".name", "documents.yml"]],
    ["multiple files", [".name", "data dir/input.yml", "second.yml"]],
    ["file and stdin", [".name", "data dir/input.yml", "-"], "name: Lin\n"],
    ["leading separator in first file", [".", "leading.yml", "second.yml"]],
    ["leading separator in later file", [".", "second.yml", "leading.yml"]],
    ["comments and leading separator", [".", "comment.yml", "tail.yml"]],
    ["comments and separators in both files", [".", "comment.yml", "comment.yml"]],
    ["scalar separator at file boundary", [".", "scalar.yml", "scalar.yml"]],
    ["scalar internal separator before next file", [".", "scalar-multiline.yml", "second.yml"]],
    ["repeated stdin", [".", "-", "-"], "name: Ada\n"],
    ["file and repeated stdin", [".", "second.yml", "-", "-"], "name: Lin\n"],
    ["partial output before missing file", [".", "second.yml", "missing.yml"]],
    ["YAML first mixed format detection", [".", "second.yml", "data.json"]],
    ["mixed formats with explicit JSON output", ["-o=json", ".", "second.yml", "data.json"]],
    ["multiple document identity collection", ["[.]", "documents.yml"]],
    ["multiple document length", ["length", "documents.yml"]],
    ["multiple document name collection", ["[.name]", "documents.yml"]],
    ["multiple file identity collection", ["[.]", "second.yml", "leading.yml"]],
    ["multiple file length", ["length", "second.yml", "leading.yml"]],
    ["multiple file name collection", ["[.name]", "second.yml", "leading.yml"]],
    ["multiple file generated scalar", ["42", "second.yml", "leading.yml"]],
    ["plain output flag", ["-M", ".name"], input],
    ["exit status true", ["-n", "-e", "true"]],
    ["exit status false", ["-n", "-e", "false"]],
    ["exit status null", ["-n", "--exit-status", "null"]],
    ["exit status string false", ["-n", "-e", '"false"']],
    ["exit status string null", ["-n", "-e", '"null"']],
    ["exit status empty selection", ["-n", "-e", "select(false)"]],
    ["exit status mixed results", ["-n", "-e", "true, false"]],
    ["exit status zero", ["-n", "-e", "0"]],
    ["exit status empty string", ["-n", "-e", '""']],
    ["exit status empty array", ["-n", "-e", "[]"]],
    ["unknown output format", ["-n", "-o", "unknown", "."]],
    ["unknown input format", ["-n", "-p", "unknown", "-o", "yaml", "."]],
    ["negative indentation", ["-n", "-I", "-1", "."]],
    ["invalid expression brackets", ["-n", "["]],
    ["invalid expression pipe", ["-n", ".foo |"]],
    ["invalid expression token", ["-n", "invalid()"]],
  ];

  for (const runtime of runtimes) {
    const command = runtime === "node" ? process.execPath : runtime;
    const cli = (args, text = "", cwd = fixture) => run(command, [entry, ...args], text, cwd);
    for (const args of [[".", "large.yml"], ["--help"]]) {
      const expected = await runWithClosedStdout(reference, args);
      assert.deepEqual(expected, { status: 141, stderr: "" }, "Go must exit silently on a broken pipe");
      assert.deepEqual(await runWithClosedStdout(command, [entry, ...args]), expected, `${runtime}: broken pipe ${args.join(" ")}`);
      checks += 1;
    }
    for (const [name, args, text = ""] of cases) {
      assert.deepEqual(cli(args, text), run(reference, args, text), `${runtime}: ${name}`);
      checks += 1;
    }
    // Parser diagnostic wording differs from Go; retain exact streaming and failure behavior.
    for (const [name, args, text = ""] of [
      ["partial output before malformed YAML", [".", "second.yml", "bad.yml"]],
      ["JSON first mixed format detection", [".", "data.json", "second.yml"]],
      ["mixed formats with explicit YAML output", ["-o=yaml", ".", "data.json", "second.yml"]],
      ["JSON BOM file is rejected", [".", "bom.json"]],
      ["JSON BOM stdin is rejected", ["-p=json", "-o=json", ".name"], files["bom.json"]],
    ]) {
      const expected = run(reference, args, text);
      const actual = cli(args, text);
      assert.equal(expected.status, 1, `${runtime}: ${name} oracle must fail`);
      assert.equal(actual.status, expected.status, `${runtime}: ${name} exit status`);
      assert.equal(actual.stdout, expected.stdout, `${runtime}: ${name} partial output`);
      assert.match(actual.stderr, /^Error: .+\n$/s, `${runtime}: ${name} diagnostic`);
      checks += 1;
    }
    for (const flag of ["--help", "-h"]) {
      const help = cli([flag]);
      assert.equal(help.status, 0, `${runtime}: ${flag}`);
      assert.match(help.stdout, /usage:/i);
      assert.equal(help.stderr, "");
      checks += 1;
    }
    const reported = cli(["--version"]);
    assert.equal(reported.status, 0);
    assert.ok(reported.stdout.includes(version), `${runtime}: version must match package metadata`);
    assert.equal(reported.stderr, "");
    checks += 1;
    assert.deepEqual(cli(["-C", ".name"], input), cli([".name"], input), `${runtime}: color flag is accepted without ANSI output`);
    checks += 1;
    const now = cli(["-n", "now"]);
    assert.equal(now.status, 0, now.stderr);
    assert.ok(Number.isFinite(Date.parse(now.stdout.trim())), `${runtime}: host must provide an RFC3339 timestamp`);
    checks += 1;
    for (const [name, args, message] of [
      ["unsupported flag", ["--split-exp", ".name", "."], "Error: --split-exp is not supported by this CLI\n"],
      ["unsupported input format", ["-p", "xml", "."], "Error: no support for input format\n"],
      ["unsupported output format", ["-o", "toml", "."], "Error: no support for output format\n"],
      ["unsupported eval-all", ["eval-all", "."], "Error: eval-all is not supported by this CLI\n"],
      ["unsupported ea", ["ea", "."], "Error: ea is not supported by this CLI\n"],
      ["unsupported front matter", ["--front-matter", "extract", "."], "Error: --front-matter is not supported by this CLI\n"],
      ["unsupported header preprocessing", ["--header-preprocess", "."], "Error: --header-preprocess is not supported by this CLI\n"],
      ["unsupported merge-anchor flag", ["--yaml-fix-merge-anchor-to-spec", "."], "Error: --yaml-fix-merge-anchor-to-spec is not supported by this CLI\n"],
      ["dynamic load", ["load(.path)"], /literal|dynamic|not supported/i],
    ]) {
      const rejected = cli(args, "path: data.json\n");
      assert.equal(rejected.status, 1, `${runtime}: ${name}`);
      assert.equal(rejected.stdout, "", `${runtime}: ${name} must not emit partial output`);
      if (typeof message === "string") {
        assert.equal(rejected.stderr, message, `${runtime}: ${name}`);
      } else {
        assert.match(rejected.stderr, message, `${runtime}: ${name}`);
      }
      checks += 1;
    }
    for (const [name, args] of [
      ["short in-place", ["-i", ".count += 1", "first.yml"]],
      ["long in-place", ["--inplace", ".count += 1", "first.yml"]],
      ["exit-status failure in-place", ["-i", "-e", "false", "first.yml"]],
      ["missing later file in-place", ["-i", ".count += 1", "first.yml", "missing.yml"]],
      ["multiple-file in-place", ["-i", ".count += 1", "first.yml", "second.yml"]],
    ]) {
      const goDir = join(fixture, `${runtime}-${name}-go`);
      const rustDir = join(fixture, `${runtime}-${name}-rust`);
      for (const dir of [goDir, rustDir]) {
        await mkdir(dir);
        await writeFile(join(dir, "first.yml"), "name: Ada\ncount: 2\n");
        await writeFile(join(dir, "second.yml"), "name: Grace\ncount: 3\n");
        await chmod(join(dir, "first.yml"), 0o640);
      }
      const expected = run(reference, args, "", goDir);
      assert.deepEqual(cli(args, "", rustDir), expected, `${runtime}: ${name} result`);
      if (expected.status !== 0) {
        assert.equal(await readFile(join(rustDir, "first.yml"), "utf8"), "name: Ada\ncount: 2\n", `${runtime}: ${name} must preserve the original file`);
      }
      for (const filename of ["first.yml", "second.yml"]) {
        assert.equal(await readFile(join(rustDir, filename), "utf8"), await readFile(join(goDir, filename), "utf8"), `${runtime}: ${name} ${filename} bytes`);
        assert.equal((await stat(join(rustDir, filename))).mode & 0o777, (await stat(join(goDir, filename))).mode & 0o777, `${runtime}: ${name} ${filename} permissions`);
      }
      checks += 1;
    }
    console.log(`${runtime}: installed CLI differential and host integration checks passed`);
  }
  console.log(`${checks} CLI checks passed against ${reference}`);
} finally {
  await rm(fixture, { recursive: true, force: true });
}
