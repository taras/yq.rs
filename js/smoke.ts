// Smoke test for the wasm module. Run after `scripts/build-wasm.sh`:
//   deno run --allow-read js/smoke.ts
//   bun js/smoke.ts
//   node --experimental-strip-types js/smoke.ts
import { createYq, type EvalOptions } from "./yq.ts";

const wasmPath = new URL("./yq.wasm", import.meta.url);
let bytes: Uint8Array;
if (typeof Deno !== "undefined") {
  bytes = await Deno.readFile(wasmPath);
} else {
  const { readFile } = await import("node:fs/promises");
  bytes = new Uint8Array(await readFile(wasmPath));
}

const yq = await createYq(bytes);
const cases: Array<[string, string, string, EvalOptions?]> = [
  [".a.b[0]", "a:\n  b: [1, 2]\n", "1\n"],
  [".a.b |= map(. * 2)", "a:\n  b: [1, 2] # c\n", "a:\n  b: [2, 4] # c\n"],
  ['.x as $v | {"v": $v}', "x: 1\n", "v: 1\n"],
  [".a", 'a: "x" # c\n', '"x" # c\n', { unwrapScalar: false }],
  [".a", 'a: "x" # c\n', "x\n"],
  [".a", 'a: "x"\n', '"x"\n', { outputFormat: "json" }],
  [".a", 'a: {b: [1, "2"]}\n', '{"b":[1,"2"]}\n', { outputFormat: "json", indent: 0 }],
  [".", '{"a": 1}', "a: 1\n", { inputFormat: "json" }],
];
let failed = 0;
for (const [expression, input, expected, options] of cases) {
  let actual: string;
  try {
    actual = yq.evaluate(expression, input, options);
  } catch (error) {
    actual = `ERROR: ${(error as Error).message}`;
  }
  const ok = actual === expected;
  failed += ok ? 0 : 1;
  console.log(`${ok ? "ok  " : "FAIL"} ${expression}  ->  ${JSON.stringify(actual)}`);
}
if (failed > 0) {
  console.error(`${failed} smoke case(s) failed`);
  // deno-lint-ignore no-explicit-any
  (globalThis as any).process?.exit?.(1) ?? (globalThis as any).Deno?.exit?.(1);
}
