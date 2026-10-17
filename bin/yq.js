#!/usr/bin/env node
import { readFile, writeFile, stat } from "node:fs/promises";
import { extname } from "node:path";
import { yq } from "./inline.js";

const metadata = JSON.parse(await readFile(new URL("../package.json", import.meta.url), "utf8"));
const referenceVersion = "v4.54.1 (a814ff5)";
const help = `yq ${metadata.version} — YAML and JSON queries (Go yq ${referenceVersion})

Usage: yq [eval|e] [flags] <expression> [files...]
Read stdin when no file is given or a file is '-'. Evaluate files independently.

  --expression EXPR       Set the expression explicitly
  --from-file FILE        Read the expression from a file
  -i, --inplace           Write all results into the first input file
  -p, --input-format FMT  yaml/y or json/j (default: filename extension)
  -o, --output-format FMT yaml/y or json/j (default: filename extension)
  -I, --indent N          Indentation (default: 2)
  -n, --null-input        Evaluate once without reading input
  -r, --unwrapScalar     Print scalar values bare (YAML default; =false disables)
  -e, --exit-status       Exit 1 if all results are null/false or there are no results
  -N, --no-doc            Omit YAML document separators
  -M, --no-colors         Accepted; output has no colors
  -C, --colors            Accepted; output has no colors
  -h, --help              Show this help
  -V, --version           Show package and reference yq versions

Environment variables and the clock are supplied by Node/Bun. File loading
supports literal paths in load, load_str, load_props and load_base64 only.

Unsupported: eval-all/ea, dynamic load paths, split-exp, front-matter,
header-preprocess, yaml-fix-merge-anchor-to-spec, and document formats other
than YAML/JSON. filename is empty; fileIndex is the input position.
`;

function flagBoolean(flag, value) {
  if (value === undefined || value === "true") {
    return true;
  }
  if (value === "false") {
    return false;
  }
  throw new Error(`invalid argument "${value}" for "${flag}" flag: invalid syntax`);
}

function parseArgs(args) {
  const config = { options: {}, positional: [], inplace: false, exitStatus: false };
  const booleanFlags = new Map([
    ["n", "nullInput"], ["null-input", "nullInput"],
    ["r", "unwrapScalar"], ["unwrapScalar", "unwrapScalar"],
    ["N", "noDoc"], ["no-doc", "noDoc"],
    ["i", "inplace"], ["inplace", "inplace"],
    ["e", "exitStatus"], ["exit-status", "exitStatus"],
    ["M", "colors"], ["C", "colors"], ["no-colors", "colors"], ["colors", "colors"],
    ["h", "help"], ["help", "help"], ["V", "version"], ["version", "version"],
  ]);
  const valueFlags = new Map([
    ["p", "inputFormat"], ["input-format", "inputFormat"],
    ["o", "outputFormat"], ["output-format", "outputFormat"],
    ["I", "indent"], ["indent", "indent"],
    ["expression", "expression"], ["from-file", "fromFile"],
  ]);
  const deferred = new Set(["s", "split-exp", "split-exp-file", "f", "front-matter", "header-preprocess", "yaml-fix-merge-anchor-to-spec"]);
  let positionalOnly = false;
  for (let index = 0; index < args.length; index += 1) {
    const argument = args[index];
    if (argument === "--" && !positionalOnly) {
      positionalOnly = true;
      continue;
    }
    if (positionalOnly || argument === "-" || !argument.startsWith("-")) {
      config.positional.push(argument);
      continue;
    }
    const long = argument.startsWith("--");
    const flagText = argument.slice(long ? 2 : 1);
    let offset = 0;
    do {
      const equal = flagText.indexOf("=", offset);
      const name = long ? flagText.slice(0, equal < 0 ? undefined : equal) : flagText[offset];
      const spelling = `${long ? "--" : "-"}${name}`;
      let value = equal === offset + name.length ? flagText.slice(equal + 1) : undefined;
      if (deferred.has(name)) {
        throw new Error(`${spelling} is not supported by this CLI`);
      }
      if (booleanFlags.has(name)) {
        const key = booleanFlags.get(name);
        const parsed = flagBoolean(spelling, value);
        if (["nullInput", "unwrapScalar"].includes(key)) {
          config.options[key] = parsed;
        } else if (key === "noDoc") {
          config.options.printDocSeparators = !parsed;
        } else {
          config[key] = parsed;
        }
      } else if (valueFlags.has(name)) {
        if (!long && value === undefined && offset + 1 < flagText.length) {
          value = flagText.slice(offset + 1);
        }
        if (value === undefined) {
          index += 1;
          value = args[index];
        }
        if (value === undefined) {
          throw new Error(`flag needs an argument: ${spelling}`);
        }
        const key = valueFlags.get(name);
        if (["expression", "fromFile"].includes(key)) {
          config[key] = value;
        } else if (key === "indent") {
          if (!/^-?\d+$/.test(value) || !Number.isSafeInteger(Number(value))) {
            throw new Error(`invalid argument "${value}" for "${spelling}" flag: invalid syntax`);
          }
          if (Number(value) < 0) {
            throw new Error("indent must not be negative");
          }
          config.options.indent = Number(value);
        } else {
          config.options[key] = parseFormat(value, key === "inputFormat" ? "input" : "output");
        }
        break;
      } else {
        throw new Error(long ? `unknown flag: ${spelling}` : `unknown shorthand flag: '${name}' in ${argument}`);
      }
      if (value !== undefined) {
        break;
      }
      offset += 1;
    } while (!long && offset < flagText.length);
  }
  if (["eval", "e"].includes(config.positional[0])) {
    config.positional.shift();
  } else if (["eval-all", "ea"].includes(config.positional[0])) {
    throw new Error(`${config.positional[0]} is not supported by this CLI`);
  }
  return config;
}

function parseFormat(value, direction) {
  if (["yaml", "y"].includes(value)) {
    return "yaml";
  }
  if (["json", "j"].includes(value)) {
    return "json";
  }
  if (["auto", "a"].includes(value)) {
    return undefined;
  }
  const aliases = "yaml|y|kyaml|ky|json|j|props|p|csv|c|tsv|t|xml|x|base64|base64url|uri|toml|hcl|h|shell|s|lua|l|ini|i";
  if (!aliases.split("|").includes(value)) {
    throw new Error(`unknown format '${value}' please use [${aliases}]`);
  }
  throw new Error(`no support for ${direction} format`);
}

async function readText(path) {
  try {
    return await readFile(path, "utf8");
  } catch (error) {
    if (error.code === "ENOENT") {
      throw new Error(`open ${path}: no such file or directory`);
    }
    throw new Error(`open ${path}: ${error.message}`);
  }
}

function readString(expression, start) {
  let end = start + 1;
  while (end < expression.length) {
    if (expression[end] === "\\") {
      end += 2;
    } else if (expression[end] === '"') {
      return { end: end + 1, raw: expression.slice(start, end + 1) };
    } else {
      end += 1;
    }
  }
  return { end, raw: expression.slice(start) };
}

function skipTrivia(expression, start) {
  let index = start;
  while (index < expression.length) {
    if (/\s/.test(expression[index])) {
      index += 1;
    } else if (expression[index] === "#") {
      const newline = expression.indexOf("\n", index);
      index = newline < 0 ? expression.length : newline + 1;
    } else {
      break;
    }
  }
  return index;
}

async function preloadFiles(expression) {
  const files = Object.create(null);
  const operators = new Set(["load", "load_str", "load_props", "load_base64"]);
  for (let index = 0; index < expression.length;) {
    index = skipTrivia(expression, index);
    if (expression[index] === '"') {
      index = readString(expression, index).end;
      continue;
    }
    const identifier = /^[A-Za-z_][A-Za-z_0-9]*/.exec(expression.slice(index));
    if (!identifier) {
      index += 1;
      continue;
    }
    const name = identifier[0];
    const before = expression[index - 1];
    index += name.length;
    const open = skipTrivia(expression, index);
    if (!operators.has(name) || before === "." || before === "$" || expression[open] !== "(") {
      continue;
    }
    const start = skipTrivia(expression, open + 1);
    const literal = expression[start] === '"' ? readString(expression, start) : undefined;
    let path;
    try {
      path = literal && JSON.parse(literal.raw);
    } catch {
      // Interpolation is valid yq syntax but is a dynamic filename.
    }
    if (typeof path !== "string" || expression[skipTrivia(expression, literal.end)] !== ")") {
      throw new Error(`${name} requires a string-literal path in this CLI; dynamic load paths are not supported`);
    }
    try {
      files[path] = await readText(path);
    } catch {
      // Unreached loads must not turn a successful expression into an error.
    }
    index = literal.end;
  }
  return files;
}

async function main() {
  const config = parseArgs(process.argv.slice(2));
  if (config.help || (config.positional[0] === "help" && config.expression === undefined && config.fromFile === undefined)) {
    process.stdout.write(help);
    return;
  }
  if (config.version) {
    process.stdout.write(`yq ${metadata.version} (Go yq ${referenceVersion})\n`);
    return;
  }
  if (config.fromFile !== undefined) {
    config.expression = await readText(config.fromFile);
  }
  if (config.expression === undefined) {
    const first = config.positional[0];
    const isFile = first !== undefined && (first === "-" || await stat(first).then(result => result.isFile(), () => false));
    config.expression = isFile || first === undefined ? "." : config.positional.shift();
  }
  const paths = config.positional;
  if (config.inplace && (paths.length === 0 || paths[0] === "-")) {
    throw new Error("write in place flag only applicable when giving an expression and at least one file");
  }
  if (config.options.nullInput && paths.length > 0) {
    throw new Error("cannot pass files in when using null-input flag");
  }
  config.options.env = process.env;
  config.options.now = new Date().toISOString();
  config.options.files = await preloadFiles(config.expression);
  let stdinConsumed = false;
  const readStdin = async () => {
    if (stdinConsumed) {
      return "";
    }
    stdinConsumed = true;
    process.stdin.setEncoding("utf8");
    let text = "";
    for await (const chunk of process.stdin) {
      text += chunk;
    }
    return text;
  };
  const detectedFormat = extname(paths[0] ?? "-").toLowerCase() === ".json" ? "json" : "yaml";
  const options = { ...config.options };
  options.inputFormat ??= detectedFormat;
  options.outputFormat ??= config.options.inputFormat === undefined ? detectedFormat : "yaml";
  const inputs = [];
  let inputError;
  for (const path of options.nullInput ? [] : paths.length === 0 ? ["-"] : paths) {
    try {
      inputs.push(path === "-" ? await readStdin() : await readText(path));
    } catch (error) {
      inputError = error;
      break;
    }
  }
  let result;
  try {
    result = yq.evaluateSequenceWithStatus(config.expression, inputs, options);
  } catch (error) {
    if (!config.inplace && error.output) {
      process.stdout.write(error.output);
    }
    throw error;
  }
  const failure = inputError ?? (config.exitStatus && !result.matched ? new Error("no matches found") : undefined);
  if (failure) {
    if (!config.inplace) {
      process.stdout.write(result.output);
    }
    throw failure;
  }
  if (config.inplace) {
    await writeFile(paths[0], result.output);
  } else {
    process.stdout.write(result.output);
  }
}

try {
  await main();
} catch (error) {
  const message = error.message.replace(/^(\d+:\d+:) invalid input text /, "$1 lexer: invalid input text ");
  process.stderr.write(`Error: ${message}\n`);
  process.exitCode = 1;
}
