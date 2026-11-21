import { cpSync, existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";

const root = process.cwd();
const confPath = join(root, "src-tauri", "tauri.conf.json");
const conf = JSON.parse(readFileSync(confPath, "utf8"));
const identifier = conf.identifier || "com.zerosmb.app";
const packagePath = identifier.replaceAll(".", "/");

const genAndroid = join(root, "src-tauri", "gen", "android");
if (!existsSync(genAndroid)) {
  console.error("Android project not found. Run `npx tauri android init` first.");
  process.exit(1);
}

const pluginSource = join(root, "android", "ZeroSmbPlugin.kt");
const pluginTarget = join(
  genAndroid,
  "app",
  "src",
  "main",
  "java",
  packagePath,
  "ZeroSmbPlugin.kt",
);
mkdirSync(dirname(pluginTarget), { recursive: true });
cpSync(pluginSource, pluginTarget);
console.log(`Copied Kotlin plugin to ${pluginTarget}`);

const progressServiceSource = join(root, "android", "ZeroSmbProgressService.kt");
const progressServiceTarget = join(
  genAndroid,
  "app",
  "src",
  "main",
  "java",
  packagePath,
  "ZeroSmbProgressService.kt",
);
cpSync(progressServiceSource, progressServiceTarget);
console.log(`Copied progress service to ${progressServiceTarget}`);

const iconsSource = join(root, "android", "icons");
for (const density of ["mdpi", "hdpi", "xhdpi", "xxhdpi", "xxxhdpi"]) {
  const destination = join(
    genAndroid,
    "app",
    "src",
    "main",
    "res",
    `mipmap-${density}`,
  );
  mkdirSync(destination, { recursive: true });
  for (const icon of [
    "ic_launcher.png",
    "ic_launcher_round.png",
    "ic_launcher_foreground.png",
  ]) {
    cpSync(join(iconsSource, `mipmap-${density}`, icon), join(destination, icon));
  }
}
console.log("Copied Android launcher icons");

const adaptiveSource = join(root, "android", "res", "mipmap-anydpi-v26");
const adaptiveTarget = join(
  genAndroid,
  "app",
  "src",
  "main",
  "res",
  "mipmap-anydpi-v26",
);
mkdirSync(adaptiveTarget, { recursive: true });
for (const icon of ["ic_launcher.xml", "ic_launcher_round.xml"]) {
  cpSync(join(adaptiveSource, icon), join(adaptiveTarget, icon));
}
console.log("Copied Android adaptive icons");

const drawableSource = join(root, "android", "res", "drawable");
const drawableTarget = join(genAndroid, "app", "src", "main", "res", "drawable");
mkdirSync(drawableTarget, { recursive: true });
for (const drawable of ["ic_stat_zerosmb.xml", "ic_launcher_background.xml"]) {
  cpSync(join(drawableSource, drawable), join(drawableTarget, drawable));
}
console.log("Copied Android drawable icons");
const wrapperPath = join(genAndroid, "gradle", "wrapper", "gradle-wrapper.properties");
if (existsSync(wrapperPath)) {
  let wrapper = readFileSync(wrapperPath, "utf8");
  wrapper = wrapper.replace(
    /^distributionUrl=.*$/m,
    "distributionUrl=https\\://github.com/gradle/gradle-distributions/releases/download/v9.6.1/gradle-9.6.1-bin.zip",
  );
  if (process.env.GRADLE_DIST_ZIP) {
    wrapper = wrapper.replace(
      /^distributionUrl=.*$/m,
      `distributionUrl=file\\://${process.env.GRADLE_DIST_ZIP}`,
    );
  }
  if (!wrapper.includes("networkTimeout=")) {
    wrapper += `${wrapper.endsWith("\n") ? "" : "\n"}networkTimeout=120000\n`;
  }
  writeFileSync(wrapperPath, wrapper);
  console.log("Configured Gradle wrapper mirror and timeout");
}
const gradleProperties = join(genAndroid, "gradle.properties");
if (existsSync(gradleProperties)) {
  let properties = readFileSync(gradleProperties, "utf8");
  if (!properties.includes("kotlin.compiler.execution.strategy=")) {
    properties += `${properties.endsWith("\n") ? "" : "\n"}kotlin.compiler.execution.strategy=in-process\n`;
    writeFileSync(gradleProperties, properties);
    console.log("Configured in-process Kotlin compiler");
  }
}
const aarSource = join(root, "android", "libs", "libzt-release.aar");
if (!existsSync(aarSource)) {
  console.error("libzt AAR not found: android/libs/libzt-release.aar");
  process.exit(1);
}
const libsDir = join(genAndroid, "app", "libs");
mkdirSync(libsDir, { recursive: true });
cpSync(aarSource, join(libsDir, "libzt-release.aar"));
console.log("Copied libzt AAR");

const shimSource = join(root, "android", "libs", "arm64-v8a", "libztShim.so");
if (!existsSync(shimSource)) {
  console.error("libzt JNI shim not found: android/libs/arm64-v8a/libztShim.so");
  process.exit(1);
}
const jniDir = join(genAndroid, "app", "src", "main", "jniLibs", "arm64-v8a");
mkdirSync(jniDir, { recursive: true });
cpSync(shimSource, join(jniDir, "libztShim.so"));
console.log("Copied libzt JNI shim");

const gradlePath = join(genAndroid, "app", "build.gradle.kts");
let gradle = readFileSync(gradlePath, "utf8");
if (!gradle.includes("libzt-release.aar")) {
  gradle = gradle.replace(
    /(dependencies\s*\{)/,
    `$1\n    implementation(files("libs/libzt-release.aar"))`,
  );
  writeFileSync(gradlePath, gradle);
  console.log("Added libzt AAR dependency");
}
const keystorePropsPath = join(root, "android", "keystore.properties");
const signing = {
  storeFile: join(process.env.USERPROFILE || "", ".android", "debug.keystore"),
  storePassword: "android",
  keyAlias: "androiddebugkey",
  keyPassword: "android",
};
if (existsSync(keystorePropsPath)) {
  for (const line of readFileSync(keystorePropsPath, "utf8").split(/\r?\n/)) {
    const index = line.indexOf("=");
    if (index > 0) {
      signing[line.slice(0, index).trim()] = line.slice(index + 1).trim();
    }
  }
}
const signingBlock =
  `signingConfigs {\n` +
  `        create("zerosmbRelease") {\n` +
  `            storeFile = file(${JSON.stringify(signing.storeFile)})\n` +
  `            storePassword = ${JSON.stringify(signing.storePassword)}\n` +
  `            keyAlias = ${JSON.stringify(signing.keyAlias)}\n` +
  `            keyPassword = ${JSON.stringify(signing.keyPassword)}\n` +
  `        }\n` +
  `    }`;
function removeSigningConfigs(text) {
  let result = text;
  while (true) {
    const start = result.indexOf("    signingConfigs {");
    if (start < 0) return result;
    let index = result.indexOf("{", start);
    let depth = 0;
    for (; index < result.length; index += 1) {
      if (result[index] === "{") depth += 1;
      if (result[index] === "}") {
        depth -= 1;
        if (depth === 0) {
          index += 1;
          break;
        }
      }
    }
    result = result.slice(0, start) + result.slice(index);
  }
}
gradle = removeSigningConfigs(gradle);
gradle = gradle.replace(/android\s*\{/, `android {\n    ${signingBlock}`);
if (!gradle.includes("signingConfig = signingConfigs.getByName")) {
  gradle = gradle.replace(
    /(getByName\("release"\)\s*\{)/,
    `$1\n            signingConfig = signingConfigs.getByName("zerosmbRelease")`,
  );
}
writeFileSync(gradlePath, gradle);
console.log("Configured release signing");
if (!gradle.includes("useLegacyPackaging")) {
  gradle = gradle.replace(
    /android\s*\{/,
    `android {\n    packaging {\n        jniLibs {\n            useLegacyPackaging = true\n        }\n    }`,
  );
  writeFileSync(gradlePath, gradle);
  console.log("Enabled legacy native library packaging");
}

const proguardPath = join(genAndroid, "app", "zerotier.pro");
writeFileSync(
  proguardPath,
  [
    "-keep class com.zerotier.sockets.** { *; }",
    "-keep class com.zerosmb.app.ZeroSmbPlugin { *; }",
    "-keepclasseswithmembernames class * { native <methods>; }",
    "",
  ].join("\n"),
);
console.log("Wrote ProGuard keep rules for libzt");

const manifestPath = join(genAndroid, "app", "src", "main", "AndroidManifest.xml");
let manifest = readFileSync(manifestPath, "utf8");

const permissions = [
  'android.permission.INTERNET',
  'android.permission.ACCESS_NETWORK_STATE',
  'android.permission.ACCESS_LOCAL_NETWORK',
  'android.permission.READ_MEDIA_IMAGES',
  'android.permission.READ_MEDIA_VIDEO',
  'android.permission.READ_MEDIA_VISUAL_USER_SELECTED',
  'android.permission.REQUEST_INSTALL_PACKAGES',
  'android.permission.POST_NOTIFICATIONS',
  'android.permission.FOREGROUND_SERVICE',
  'android.permission.FOREGROUND_SERVICE_DATA_SYNC',
  'android.permission.WAKE_LOCK',
];

const requiredBlocks = [
  ...permissions.map((name) => `<uses-permission android:name="${name}" />`),
  '<uses-permission android:name="android.permission.READ_EXTERNAL_STORAGE" android:maxSdkVersion="32" />',
  '<uses-permission android:name="android.permission.WRITE_EXTERNAL_STORAGE" android:maxSdkVersion="28" />',
];

for (const block of requiredBlocks) {
  if (!manifest.includes(block)) {
    manifest = manifest.replace(/(<application\b)/, `${block}\n    $1`);
  }
}

if (!manifest.includes("android:usesCleartextTraffic")) {
  manifest = manifest.replace(
    /<application\b/,
    '<application\n        android:usesCleartextTraffic="true"',
  );
}

if (!manifest.includes("android:roundIcon")) {
  manifest = manifest.replace(
    'android:icon="@mipmap/ic_launcher"',
    'android:icon="@mipmap/ic_launcher"\n        android:roundIcon="@mipmap/ic_launcher_round"',
  );
}

const progressService =
  '<service android:name=".ZeroSmbProgressService" android:exported="false" android:foregroundServiceType="dataSync" />';
if (!manifest.includes("ZeroSmbProgressService")) {
  manifest = manifest.replace("</activity>", `</activity>\n\n        ${progressService}`);
}

manifest = manifest.replace(/\s*android:extractNativeLibs="[^"]*"/g, "");

writeFileSync(manifestPath, manifest);
console.log(`Patched ${manifestPath}`);
