import { createRequire } from "node:module";
import { YqError } from "./yq.js";
import { platformTarget } from "./platform.mjs";
export { YqError } from "./yq.js";

let binding;
if (!process.versions.bun && process.env.YQ_FORCE_WASM !== "1") {
  const target = platformTarget();
  if (target) {
    try {
      binding = createRequire(import.meta.url)(`yq.rs-${target}-napi`);
    } catch {
      // Missing packages and incompatible Node-API libraries use the portable backend.
    }
  }
}
export const backend = binding ? "native" : "wasm";
const inline = binding ? undefined : await import("./inline.js");
const keys = new Set(["inputFormat", "outputFormat", "indent", "nullInput", "env", "files", "now", "unwrapScalar", "printDocSeparators"]);
function run(method, expression, input, options = {}) {
  const normalized = JSON.parse(JSON.stringify(options));
  for (const key of Object.keys(normalized)) {
    if (!keys.has(key)) {
      throw new YqError(`unknown option: ${key}`);
    }
  }
  let result;
  try {
    result = binding[method](expression, input, normalized);
  } catch (error) {
    throw new YqError(error.message);
  }
  if (result.error != null) {
    throw new YqError(result.error, result.output);
  }
  return { output: result.output, matched: result.matched };
}
export async function createYq() {
  if (!binding) {
    return inline.createYq();
  }
  return {
    evaluate(expression, input, options) {
      return run("evaluate", expression, input, options).output;
    },
    evaluateWithStatus(expression, input, options) {
      return run("evaluateWithStatus", expression, input, options);
    },
    evaluateSequenceWithStatus(expression, inputs, options) {
      return run("evaluateSequence", expression, inputs, options);
    },
  };
}
export const yq = binding ? await createYq() : inline.yq;
