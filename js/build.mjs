import { execFileSync } from "node:child_process";
import { copyFile, mkdir, readFile, rm, writeFile } from "node:fs/promises";
import { fileURLToPath } from "node:url";

const root = fileURLToPath(new URL("../", import.meta.url));
const wasm = await readFile(new URL("./yq.wasm", import.meta.url));
const module = await WebAssembly.compile(wasm);
if (WebAssembly.Module.imports(module).length !== 0) {
  throw new Error("yq.wasm must instantiate with an empty import object");
}

await rm(new URL("../dist/", import.meta.url), { recursive: true, force: true });
await mkdir(new URL("../dist/", import.meta.url), { recursive: true });
execFileSync(process.execPath, ["node_modules/typescript/bin/tsc", "--project", "js/tsconfig.json"], {
  cwd: root,
  stdio: "inherit",
});
await copyFile(new URL("./yq.wasm", import.meta.url), new URL("../dist/yq.wasm", import.meta.url));
await writeFile(new URL("../dist/inline.js", import.meta.url), `import { createYq as instantiate } from "./yq.js";
export { YqError } from "./yq.js";

const encoded = "${wasm.toString("base64")}";
// Native decoders take well under a millisecond; the atob loop over half a
// million characters costs about 30 ms and is kept only as the last resort.
const bytes =
  typeof Uint8Array.fromBase64 === "function"
    ? Uint8Array.fromBase64(encoded)
    : typeof Buffer === "function"
      ? new Uint8Array(Buffer.from(encoded, "base64"))
      : Uint8Array.from(atob(encoded), character => character.charCodeAt(0));
const module = WebAssembly.compile(bytes);

export async function createYq() {
  return instantiate(await module);
}

export const yq = await createYq();
`);
await writeFile(new URL("../dist/inline.d.ts", import.meta.url), `import type { Yq } from "./yq.js";
export { YqError } from "./yq.js";
export type { EvalOptions, Format, Yq } from "./yq.js";
export declare function createYq(): Promise<Yq>;
export declare const yq: Yq;
`);
