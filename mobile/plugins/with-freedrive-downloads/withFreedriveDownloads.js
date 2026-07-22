const {
  withDangerousMod,
  createRunOncePlugin,
} = require("expo/config-plugins");
const fs = require("fs");
const path = require("path");

const PLUGIN_NAME = "with-freedrive-downloads";
const PACKAGE_DIR = "app/src/main/java/com/freedrive/mobile";

function copyKotlinSources(androidProjectRoot) {
  const destDir = path.join(androidProjectRoot, PACKAGE_DIR);
  fs.mkdirSync(destDir, { recursive: true });
  const srcDir = __dirname;
  for (const name of ["DownloadsModule.kt", "DownloadsPackage.kt"]) {
    fs.copyFileSync(path.join(srcDir, name), path.join(destDir, name));
  }
}

function ensurePackageRegistered(mainAppPath) {
  if (!fs.existsSync(mainAppPath)) {
    throw new Error(`MainApplication not found at ${mainAppPath}`);
  }
  let contents = fs.readFileSync(mainAppPath, "utf8");
  if (contents.includes("DownloadsPackage()")) {
    return;
  }

  if (contents.includes("// add(MyReactNativePackage())")) {
    contents = contents.replace(
      "// add(MyReactNativePackage())",
      "add(DownloadsPackage())",
    );
  } else if (
    /PackageList\(this\)\.packages\.apply\s*\{/.test(contents)
  ) {
    contents = contents.replace(
      /PackageList\(this\)\.packages\.apply\s*\{/,
      (match) => `${match}\n          add(DownloadsPackage())`,
    );
  } else {
    throw new Error(
      "Could not find PackageList packages.apply block to register DownloadsPackage",
    );
  }

  fs.writeFileSync(mainAppPath, contents);
}

function withFreedriveDownloads(config) {
  return withDangerousMod(config, [
    "android",
    async (cfg) => {
      const androidRoot = cfg.modRequest.platformProjectRoot;
      copyKotlinSources(androidRoot);
      ensurePackageRegistered(
        path.join(androidRoot, PACKAGE_DIR, "MainApplication.kt"),
      );
      return cfg;
    },
  ]);
}

module.exports = createRunOncePlugin(
  withFreedriveDownloads,
  PLUGIN_NAME,
  "1.0.0",
);
