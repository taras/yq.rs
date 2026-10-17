#!/usr/bin/env node
import { execFileSync } from "node:child_process";
import { createRequire } from "node:module";
import { constants } from "node:os";
import { platformTarget } from "./platform.mjs";

let binary;
if (process.env.YQ_FORCE_WASM !== "1") {
  const target = platformTarget();
  if (target) {
    try {
      binary = createRequire(import.meta.url).resolve(`yq.rs-${target}/yq${process.platform === "win32" ? ".exe" : ""}`);
    } catch (error) {
      if (error.code !== "MODULE_NOT_FOUND") {
        throw error;
      }
    }
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
