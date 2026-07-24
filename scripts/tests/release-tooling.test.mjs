import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { dirname } from "node:path";
import { fileURLToPath } from "node:url";
import test from "node:test";

import { configureBundleUpdaterSigningKey } from "../tauri-signing-env.mjs";

const fixturePath = fileURLToPath(import.meta.url);

test("maps a readable key path to the variable consumed by Tauri build", () => {
  const environment = {
    TAURI_SIGNING_PRIVATE_KEY_PATH: fixturePath,
    TAURI_SIGNING_PRIVATE_KEY_PASSWORD: "not-a-real-password",
  };

  assert.equal(configureBundleUpdaterSigningKey(environment), "path");
  assert.equal(environment.TAURI_SIGNING_PRIVATE_KEY, fixturePath);
  assert.equal("TAURI_SIGNING_PRIVATE_KEY_PATH" in environment, false);
  assert.equal(
    environment.TAURI_SIGNING_PRIVATE_KEY_PASSWORD,
    "not-a-real-password",
  );
});

test("does not replace an explicitly provided inline key", () => {
  const environment = { TAURI_SIGNING_PRIVATE_KEY: "inline-test-value" };

  assert.equal(configureBundleUpdaterSigningKey(environment), "inline");
  assert.equal(environment.TAURI_SIGNING_PRIVATE_KEY, "inline-test-value");
});

test("rejects ambiguous or non-file key configuration without echoing paths", () => {
  assert.throws(
    () =>
      configureBundleUpdaterSigningKey({
        TAURI_SIGNING_PRIVATE_KEY: "inline-test-value",
        TAURI_SIGNING_PRIVATE_KEY_PATH: fixturePath,
      }),
    /Set only one/,
  );

  const directory = dirname(fixturePath);
  assert.throws(
    () =>
      configureBundleUpdaterSigningKey({
        TAURI_SIGNING_PRIVATE_KEY_PATH: directory,
      }),
    (error) =>
      error instanceof Error
      && !error.message.includes(directory)
      && error.message.includes("readable regular file"),
  );
});

test("the Tauri build wrapper applies path mapping without reading key bytes", async () => {
  const [wrapper, signingEnvironment] = await Promise.all([
    readFile(new URL("../run-tauri.mjs", import.meta.url), "utf8"),
    readFile(new URL("../tauri-signing-env.mjs", import.meta.url), "utf8"),
  ]);
  const configure = wrapper.indexOf(
    "configureBundleUpdaterSigningKey(environment)",
  );
  const spawnTauri = wrapper.indexOf(
    "spawn(process.execPath, [tauriCli, ...tauriArguments]",
  );

  assert.match(wrapper, /tauriArguments\.includes\("build"\)/);
  assert.ok(configure >= 0, "the signing environment helper must be called");
  assert.ok(spawnTauri > configure, "path mapping must happen before Tauri starts");
  assert.doesNotMatch(signingEnvironment, /readFile(?:Sync)?\s*\(/);
});

test("DMG notarization completes and staples before distribution gates", async () => {
  const releaseScript = await readFile(
    new URL("../release-macos-local.sh", import.meta.url),
    "utf8",
  );
  const submit = releaseScript.indexOf("notarytool submit");
  const accepted = releaseScript.indexOf(".status == \"Accepted\"");
  const staple = releaseScript.indexOf('stapler staple "$dmg_bundle"');
  const validate = releaseScript.indexOf('stapler validate "$dmg_bundle"');
  const gatekeeper = releaseScript.indexOf(
    'spctl --assess --type open --context context:primary-signature "$dmg_bundle"',
  );

  assert.ok(submit >= 0, "notarytool submission is required");
  assert.ok(accepted > submit, "Apple must report Accepted");
  assert.ok(staple > accepted, "the accepted DMG must be stapled");
  assert.ok(validate > staple, "the stapled ticket must be validated");
  assert.ok(gatekeeper > validate, "Gatekeeper runs after ticket validation");
  assert.match(releaseScript, /notarytool submit[\s\S]+--wait/);
  assert.match(releaseScript, /notarytool submit[\s\S]+--output-format json/);
});
