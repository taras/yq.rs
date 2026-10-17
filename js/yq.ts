// Reference loader for the yq-wasm module. Runtime-neutral: it only uses the
// WebAssembly and TextEncoder/TextDecoder globals, so the same file works in
// Deno, Bun, Node and browsers. Reading the .wasm bytes is the caller's job.

export type Format = "yaml" | "json";

export interface EvalOptions {
  inputFormat?: Format;
  outputFormat?: Format;
  indent?: number;
  nullInput?: boolean;
  /** Backing store for env(NAME), strenv(NAME) and envsubst. */
  env?: Record<string, string>;
  /** RFC 3339 timestamp returned by `now`. */
  now?: string;
}

export interface Yq {
  evaluate(expression: string, input: string, options?: EvalOptions): string;
}

interface Exports {
  memory: WebAssembly.Memory;
  yq_alloc(len: number): number;
  yq_free(ptr: number, len: number): void;
  yq_eval(
    exprPtr: number,
    exprLen: number,
    inputPtr: number,
    inputLen: number,
    optsPtr: number,
    optsLen: number,
  ): number;
  yq_result_free(header: number): void;
}

export class YqError extends Error {
  constructor(message: string) {
    super(message);
    this.name = "YqError";
  }
}

export async function createYq(source: BufferSource | WebAssembly.Module): Promise<Yq> {
  const module = source instanceof WebAssembly.Module ? source : await WebAssembly.compile(source);
  const instance = await WebAssembly.instantiate(module, {});
  const exports = instance.exports as unknown as Exports;
  const encoder = new TextEncoder();
  const decoder = new TextDecoder();

  // The memory buffer is detached whenever wasm memory grows, so it must be
  // re-read after every call into the module.
  const write = (text: string): [number, number] => {
    const bytes = encoder.encode(text);
    const ptr = exports.yq_alloc(bytes.length);
    new Uint8Array(exports.memory.buffer, ptr, bytes.length).set(bytes);
    return [ptr, bytes.length];
  };

  return {
    evaluate(expression, input, options = {}) {
      const [exprPtr, exprLen] = write(expression);
      const [inputPtr, inputLen] = write(input);
      const [optsPtr, optsLen] = write(JSON.stringify(options));
      const header = exports.yq_eval(exprPtr, exprLen, inputPtr, inputLen, optsPtr, optsLen);
      try {
        const view = new DataView(exports.memory.buffer, header, 12);
        const status = view.getUint32(0, true);
        const ptr = view.getUint32(4, true);
        const len = view.getUint32(8, true);
        const text = decoder.decode(new Uint8Array(exports.memory.buffer, ptr, len));
        if (status !== 0) {
          throw new YqError(text);
        }
        return text;
      } finally {
        exports.yq_result_free(header);
        exports.yq_free(exprPtr, exprLen);
        exports.yq_free(inputPtr, inputLen);
        exports.yq_free(optsPtr, optsLen);
      }
    },
  };
}
