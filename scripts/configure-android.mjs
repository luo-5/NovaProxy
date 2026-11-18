#!/usr/bin/env node
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const projectRoot = path.resolve(__dirname, "..");
const androidAppDir = path.join(projectRoot, "src-tauri", "gen", "android", "app");
const manifestPath = path.join(androidAppDir, "src", "main", "AndroidManifest.xml");
const buildGradlePath = path.join(androidAppDir, "build.gradle.kts");

if (!fs.existsSync(manifestPath) || !fs.existsSync(buildGradlePath)) {
  console.error("Android project not initialized yet. Run `pnpm tauri android init` first.");
  process.exit(1);
}

// 1. Ensure AndroidManifest.xml enables extractNativeLibs=true (required for Android 10+ W^X
//    execution of bundled core .so binaries inside nativeLibraryDir) and handles novaproxy:// / clash:// links.
let manifest = fs.readFileSync(manifestPath, "utf8");
if (!manifest.includes("android:extractNativeLibs")) {
  manifest = manifest.replace(
    /<application\b/,
    '<application\n        android:extractNativeLibs="true"'
  );
} else {
  manifest = manifest.replace(
    /android:extractNativeLibs="(true|false)"/,
    'android:extractNativeLibs="true"'
  );
}

const intentSchemes = ["novaproxy", "clash"];
for (const scheme of intentSchemes) {
  if (!manifest.includes(`android:scheme="${scheme}"`)) {
    const schemeFilter = `
            <intent-filter>
                <action android:name="android.intent.action.VIEW" />
                <category android:name="android.intent.category.DEFAULT" />
                <category android:name="android.intent.category.BROWSABLE" />
                <data android:scheme="${scheme}" />
            </intent-filter>`;
    manifest = manifest.replace(/<\/activity>/, `${schemeFilter}\n        </activity>`);
  }
}

fs.writeFileSync(manifestPath, manifest, "utf8");
console.log("Updated AndroidManifest.xml cleanly.");

// 2. Patch app/build.gradle.kts so:
//    - compileSdk / targetSdk align with installed Android 36 SDK platform
//    - native jniLibs extract to disk (useLegacyPackaging = true)
//    - ABI packaging matches our 64-bit targets (arm64-v8a + x86_64) where all 3 proxy cores ship
//    - desktop executables (.exe/.dll) are excluded from APK assets
let gradle = fs.readFileSync(buildGradlePath, "utf8");

gradle = gradle.replace(/compileSdk\s*=\s*37/, "compileSdk = 36");
gradle = gradle.replace(/targetSdk\s*=\s*37/, "targetSdk = 36");

if (!gradle.includes("abiFilters")) {
  gradle = gradle.replace(
    /defaultConfig\s*\{/,
    `defaultConfig {\n        ndk {\n            abiFilters += listOf("arm64-v8a", "x86_64")\n        }`
  );
}

if (!gradle.includes("useLegacyPackaging")) {
  const packagingSnippet = `
    packaging {
        jniLibs {
            useLegacyPackaging = true
        }
    }
    androidResources {
        ignoreAssetsPattern = "!.svn:!.git:!.ds_store:!*.scc:.*:!CVS:!thumbs.db:!picasa.ini:!*~:*.exe:*.dll:*.bak:*.zip:*.gz"
    }
`;
  if (gradle.includes("android {")) {
    gradle = gradle.replace(/android\s*\{/, `android {\n${packagingSnippet}`);
  }
} else if (!gradle.includes("ignoreAssetsPattern")) {
  gradle = gradle.replace(
    /android\s*\{/,
    `android {\n    androidResources {\n        ignoreAssetsPattern = "!.svn:!.git:!.ds_store:!*.scc:.*:!CVS:!thumbs.db:!picasa.ini:!*~:*.exe:*.dll:*.bak:*.zip:*.gz"\n    }\n`
  );
}

fs.writeFileSync(buildGradlePath, gradle, "utf8");
console.log("Updated build.gradle.kts cleanly.");

// 3. Allow Windows non-ASCII workspace paths (e.g. C:\Users\...\VPN客户端开发) cleanly
const gradlePropsPath = path.join(projectRoot, "src-tauri", "gen", "android", "gradle.properties");
if (fs.existsSync(gradlePropsPath)) {
  let props = fs.readFileSync(gradlePropsPath, "utf8");
  if (!props.includes("android.overridePathCheck=true")) {
    props = `${props.trimEnd()}\nandroid.overridePathCheck=true\n`;
    fs.writeFileSync(gradlePropsPath, props, "utf8");
    console.log("Updated gradle.properties cleanly.");
  }
}
