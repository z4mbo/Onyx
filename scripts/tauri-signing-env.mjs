import {
  accessSync,
  constants,
  realpathSync,
  statSync,
} from "node:fs";

/**
 * Tauri CLI 2.11 build/bundle reads TAURI_SIGNING_PRIVATE_KEY and accepts
 * either key contents or a path. TAURI_SIGNING_PRIVATE_KEY_PATH is a direct
 * option only for `tauri signer sign`, so translate it without reading the
 * secret into this Node process.
 */
export function configureBundleUpdaterSigningKey(environment) {
  const inlineKey = environment.TAURI_SIGNING_PRIVATE_KEY;
  const configuredPath = environment.TAURI_SIGNING_PRIVATE_KEY_PATH;

  if (inlineKey && configuredPath) {
    throw new Error(
      "Set only one of TAURI_SIGNING_PRIVATE_KEY or TAURI_SIGNING_PRIVATE_KEY_PATH.",
    );
  }

  if (!configuredPath) {
    return inlineKey ? "inline" : "none";
  }

  let canonicalPath;
  try {
    canonicalPath = realpathSync(configuredPath);
    const metadata = statSync(canonicalPath);
    accessSync(canonicalPath, constants.R_OK);
    if (!metadata.isFile()) {
      throw new Error("not a regular file");
    }
  } catch {
    throw new Error(
      "TAURI_SIGNING_PRIVATE_KEY_PATH must point to a readable regular file.",
    );
  }

  environment.TAURI_SIGNING_PRIVATE_KEY = canonicalPath;
  delete environment.TAURI_SIGNING_PRIVATE_KEY_PATH;
  return "path";
}
