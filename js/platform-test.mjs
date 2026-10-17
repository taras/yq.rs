import assert from "node:assert/strict";
import { platformTarget } from "./platform.mjs";

const glibc = () => ({ header: { glibcVersionRuntime: "2.28" } });
const musl = () => ({ header: {} });
for (const [os, arch, report, expected] of [
  ["darwin", "arm64", glibc, "darwin-arm64"],
  ["darwin", "x64", glibc, "darwin-x64"],
  ["linux", "x64", glibc, "linux-x64-gnu"],
  ["linux", "arm64", glibc, "linux-arm64-gnu"],
  ["linux", "x64", musl, "linux-x64-musl"],
  ["win32", "x64", glibc, "win32-x64"],
  ["linux", "arm64", musl, undefined],
  ["win32", "arm64", glibc, undefined],
  ["freebsd", "x64", glibc, undefined],
]) {
  assert.equal(platformTarget(os, arch, report), expected);
}
console.log("Native platform selection passed");
