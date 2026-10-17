export function platformTarget(platform = process.platform, arch = process.arch, report = () => process.report?.getReport()) {
  if (platform === "darwin" && (arch === "arm64" || arch === "x64")) {
    return `${platform}-${arch}`;
  }
  if (platform === "win32" && arch === "x64") {
    return "win32-x64";
  }
  if (platform === "linux" && (arch === "arm64" || arch === "x64")) {
    const glibc = report()?.header?.glibcVersionRuntime;
    if (glibc) {
      return `linux-${arch}-gnu`;
    }
    if (arch === "x64") {
      return "linux-x64-musl";
    }
  }
  return undefined;
}
