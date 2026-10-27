#!/usr/bin/env node
"use strict";

// Runs the native `simsapa` binary from the platform package npm installed
// alongside this one (an optionalDependency whose `os`/`cpu` match). Kept
// dependency-free. See docs/backend-cli-and-npm-package.md.

const child_process = require("child_process");

// "<process.platform> <process.arch>" → platform package.
// Windows on ARM runs the x64 binary under the system's x64 emulation, which
// is why cli-win32-x64 also lists `arm64` in its `cpu` field.
const PLATFORM_PACKAGES = {
  "linux x64": "@simsapa/cli-linux-x64",
  "linux arm64": "@simsapa/cli-linux-arm64",
  "darwin arm64": "@simsapa/cli-darwin-arm64",
  "win32 x64": "@simsapa/cli-win32-x64",
  "win32 arm64": "@simsapa/cli-win32-x64",
};

const SUPPORTED_PLATFORMS = ["linux-x64", "linux-arm64", "darwin-arm64", "win32-x64"];

function platform_package(platform, arch) {
  return PLATFORM_PACKAGES[`${platform} ${arch}`] || null;
}

function binary_path_in_package(pkg, platform) {
  return `${pkg}/bin/simsapa${platform === "win32" ? ".exe" : ""}`;
}

function unsupported_platform_message(platform, arch) {
  return [
    `simsapa: there is no simsapa binary for ${platform}-${arch}.`,
    `Supported platforms: ${SUPPORTED_PLATFORMS.join(", ")}.`,
  ].join("\n");
}

function missing_package_message(pkg, platform, arch) {
  return [
    `simsapa: the package ${pkg}, which holds the simsapa binary for ${platform}-${arch}, is not installed.`,
    `Supported platforms: ${SUPPORTED_PLATFORMS.join(", ")}.`,
    "It is an optional dependency of @simsapa/cli. Reinstall without --omit=optional (or --no-optional):",
    "  npm install -g @simsapa/cli",
  ].join("\n");
}

// Returns { binary } or { error }. `resolve` is require.resolve, passed in so
// the tests can stand in for an installed (or missing) platform package.
function find_binary(platform, arch, resolve) {
  const pkg = platform_package(platform, arch);
  if (pkg === null) {
    return { error: unsupported_platform_message(platform, arch) };
  }
  try {
    return { binary: resolve(binary_path_in_package(pkg, platform)) };
  } catch (e) {
    return { error: missing_package_message(pkg, platform, arch) };
  }
}

function main() {
  const found = find_binary(process.platform, process.arch, require.resolve);
  if (found.error) {
    process.stderr.write(found.error + "\n");
    process.exit(1);
  }

  // Ctrl-C reaches the binary directly: the terminal signals the whole
  // foreground process group. Without a listener Node would exit at once and
  // return the shell prompt while the binary is still cleaning up (`setup`
  // deletes its partial download). The listener defers that until spawnSync
  // returns, and the binary's own exit status (130) is then forwarded.
  const ignore_sigint = () => {};
  process.on("SIGINT", ignore_sigint);

  const result = child_process.spawnSync(found.binary, process.argv.slice(2), { stdio: "inherit" });

  process.removeListener("SIGINT", ignore_sigint);

  if (result.error) {
    process.stderr.write(`simsapa: could not run ${found.binary}: ${result.error.message}\n`);
    process.exit(1);
  }
  if (result.signal) {
    // Die of the same signal, so the caller sees what the binary saw.
    process.kill(process.pid, result.signal);
    process.exit(1);
  }
  process.exit(result.status === null ? 1 : result.status);
}

if (require.main === module) {
  main();
}

module.exports = {
  SUPPORTED_PLATFORMS,
  platform_package,
  binary_path_in_package,
  find_binary,
};
