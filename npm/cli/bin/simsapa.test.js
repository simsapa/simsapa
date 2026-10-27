const {
  SUPPORTED_PLATFORMS,
  platform_package,
  binary_path_in_package,
  find_binary,
} = require("./simsapa.js");

describe("platform_package", () => {
  test("maps each supported platform to its package", () => {
    expect(platform_package("linux", "x64")).toBe("@simsapa/cli-linux-x64");
    expect(platform_package("linux", "arm64")).toBe("@simsapa/cli-linux-arm64");
    expect(platform_package("darwin", "arm64")).toBe("@simsapa/cli-darwin-arm64");
    expect(platform_package("win32", "x64")).toBe("@simsapa/cli-win32-x64");
  });

  test("Windows on ARM uses the x64 binary", () => {
    expect(platform_package("win32", "arm64")).toBe("@simsapa/cli-win32-x64");
  });

  test("unsupported platforms have no package", () => {
    expect(platform_package("darwin", "x64")).toBeNull();
    expect(platform_package("linux", "ia32")).toBeNull();
    expect(platform_package("freebsd", "x64")).toBeNull();
  });

  test("every supported platform has a package of the same name", () => {
    for (const p of SUPPORTED_PLATFORMS) {
      const [platform, arch] = p.split("-");
      expect(platform_package(platform, arch)).toBe(`@simsapa/cli-${p}`);
    }
  });
});

describe("binary_path_in_package", () => {
  test("adds .exe on Windows only", () => {
    expect(binary_path_in_package("@simsapa/cli-win32-x64", "win32")).toBe("@simsapa/cli-win32-x64/bin/simsapa.exe");
    expect(binary_path_in_package("@simsapa/cli-linux-x64", "linux")).toBe("@simsapa/cli-linux-x64/bin/simsapa");
  });
});

describe("find_binary", () => {
  test("resolves the binary of the installed package", () => {
    const resolve = (p) => `/prefix/node_modules/${p}`;
    expect(find_binary("linux", "x64", resolve)).toEqual({
      binary: "/prefix/node_modules/@simsapa/cli-linux-x64/bin/simsapa",
    });
  });

  test("a missing platform package says to reinstall without --omit=optional", () => {
    const resolve = (p) => {
      throw new Error(`Cannot find module '${p}'`);
    };
    const found = find_binary("darwin", "arm64", resolve);
    expect(found.binary).toBeUndefined();
    expect(found.error).toContain("@simsapa/cli-darwin-arm64");
    expect(found.error).toContain("--omit=optional");
    expect(found.error).toContain("linux-x64, linux-arm64, darwin-arm64, win32-x64");
  });

  test("an unsupported platform lists the supported ones and never resolves", () => {
    const resolve = () => {
      throw new Error("must not be called");
    };
    const found = find_binary("darwin", "x64", resolve);
    expect(found.error).toContain("darwin-x64");
    expect(found.error).toContain("linux-x64, linux-arm64, darwin-arm64, win32-x64");
    expect(found.error).not.toContain("--omit=optional");
  });
});
