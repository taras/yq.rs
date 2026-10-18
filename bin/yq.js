#!/usr/bin/env node
import { execFileSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import { constants } from "node:os";
import { platformTarget } from "./platform.mjs";

let binary;
if (process.env.YQ_FORCE_WASM !== "1") {
  const target = platformTarget();
  if (target) {
    binary = fileURLToPath(new URL(`./native/${target}/yq${process.platform === "win32" ? ".exe" : ""}`, import.meta.url));
  }
}
if (binary) {
  try {
    execFileSync(binary, process.argv.slice(2), { stdio: "inherit" });
  } catch (error) {
    if (typeof error.status === "number") {
      process.exit(error.status);
    }
    if (error.signal) {
      process.exit(128 + (constants.signals[error.signal] ?? 1));
    }
    binary = undefined;
  }
}
if (!binary) {
  await import("./cli-wasm.mjs");
}
