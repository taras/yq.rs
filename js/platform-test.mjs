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
  ["linux", "arm64", musl, "linux-arm64-musl"],
  ["win32", "arm64", glibc, "win32-arm64"],
  ["win32", "ia32", glibc, undefined],
  ["freebsd", "x64", glibc, undefined],
]) {
  assert.equal(platformTarget(os, arch, report), expected);
}
const originalGetReport = process.report.getReport;
const originalExcludeNetwork = process.report.excludeNetwork;
try {
  process.report.excludeNetwork = false;
  process.report.getReport = () => {
    assert.equal(process.report.excludeNetwork, true, "Platform detection must exclude network inspection");
    return glibc();
  };
  assert.equal(platformTarget("linux", "arm64"), "linux-arm64-gnu");
} finally {
  process.report.getReport = originalGetReport;
  process.report.excludeNetwork = originalExcludeNetwork;
}
console.log("Native platform selection passed");
