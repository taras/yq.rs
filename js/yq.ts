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
  /** Contents for `load(path)` and friends, keyed by the exact path used. */
  files?: Record<string, string>;
  /**
   * Print scalar results bare, without quotes or comments (yq's `-r`).
   * Defaults to true for YAML output and false for JSON, as yq does.
   */
  unwrapScalar?: boolean;
  /** Emit YAML document separators; false implements yq's `-N`. */
  printDocSeparators?: boolean;
}

export interface Evaluation {
  output: string;
  matched: boolean;
}

export interface Yq {
  evaluate(expression: string, input: string, options?: EvalOptions): string;
  /** Also report whether any result was neither null nor boolean false. */
  evaluateWithStatus(expression: string, input: string, options?: EvalOptions): Evaluation;
  /** Independent inputs with shared printing. Errors retain earlier output. */
  evaluateSequenceWithStatus(expression: string, inputs: string[], options?: EvalOptions): Evaluation;
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
  yq_eval_with_status: Exports["yq_eval"];
  yq_eval_sequence: Exports["yq_eval"];
  yq_result_free(header: number): void;
}

export class YqError extends Error {
  constructor(message: string, readonly output = "") {
    super(message);
    this.name = "YqError";
  }
}

export async function createYq(source: BufferSource | WebAssembly.Module): Promise<Yq> {
  const module = source instanceof WebAssembly.Module ? source : await WebAssembly.compile(source);
  const instance = await WebAssembly.instantiate(module, {});
  const exports = instance.exports as unknown as Exports;
  const encoder = new TextEncoder();
  // A result's leading U+FEFF is scalar content, not a transport BOM.
  const decoder = new TextDecoder("utf-8", { ignoreBOM: true });

  // The memory buffer is detached whenever wasm memory grows, so it must be
  // re-read after every call into the module.
  const write = (text: string): [number, number] => {
    const bytes = encoder.encode(text);
    const ptr = exports.yq_alloc(bytes.length);
    new Uint8Array(exports.memory.buffer, ptr, bytes.length).set(bytes);
    return [ptr, bytes.length];
  };

  const evaluate = (expression: string, input: string, options: EvalOptions = {}, reportStatus = false, sequence = false): Evaluation => {
    const optionsJson = JSON.stringify(options);
    const [exprPtr, exprLen] = write(expression);
    const [inputPtr, inputLen] = write(input);
    const [optsPtr, optsLen] = write(optionsJson);
    const run = sequence ? exports.yq_eval_sequence : reportStatus ? exports.yq_eval_with_status : exports.yq_eval;
    const header = run(exprPtr, exprLen, inputPtr, inputLen, optsPtr, optsLen);
    try {
      const view = new DataView(exports.memory.buffer, header, 12);
      const status = view.getUint32(0, true);
      const ptr = view.getUint32(4, true);
      const len = view.getUint32(8, true);
      const text = decoder.decode(new Uint8Array(exports.memory.buffer, ptr, len));
      if (sequence && status === 3) {
        const failure = JSON.parse(text);
        throw new YqError(failure.error, failure.output);
      }
      if (status !== 0 && !(reportStatus && status === 2)) {
        throw new YqError(text);
      }
      return { output: text, matched: status === 0 };
    } finally {
      exports.yq_result_free(header);
      exports.yq_free(exprPtr, exprLen);
      exports.yq_free(inputPtr, inputLen);
      exports.yq_free(optsPtr, optsLen);
    }
  };
  return {
    evaluate(expression, input, options = {}) {
      return evaluate(expression, input, options).output;
    },
    evaluateWithStatus(expression, input, options = {}) {
      return evaluate(expression, input, options, true);
    },
    evaluateSequenceWithStatus(expression, inputs, options = {}) {
      return evaluate(expression, JSON.stringify(Object.fromEntries(inputs.map((text, index) => [index, text]))), options, true, true);
    },
  };
}
