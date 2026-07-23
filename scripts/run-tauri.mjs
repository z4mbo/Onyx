#!/usr/bin/env node

import { existsSync, readdirSync } from "node:fs";
import { homedir } from "node:os";
import { delimiter, join, sep } from "node:path";
import { fileURLToPath } from "node:url";
import { spawn, spawnSync } from "node:child_process";

const cargoExecutable = process.platform === "win32" ? "cargo.exe" : "cargo";
const pathEntries = (process.env.PATH ?? "").split(delimiter).filter(Boolean);

function containsCargo(directory) {
  return Boolean(directory) && existsSync(join(directory, cargoExecutable));
}

function rustupToolchainBins() {
  const toolchainsDirectory = join(homedir(), ".rustup", "toolchains");
  if (!existsSync(toolchainsDirectory)) return [];

  return readdirSync(toolchainsDirectory, { withFileTypes: true })
    .filter((entry) => entry.isDirectory())
    .map((entry) => join(toolchainsDirectory, entry.name, "bin"))
    .filter(containsCargo)
    .sort((left, right) => {
      const leftStable = left.includes(`${sep}stable-`);
      const rightStable = right.includes(`${sep}stable-`);
      return Number(rightStable) - Number(leftStable) || right.localeCompare(left);
    });
}

const cargoHome = process.env.CARGO_HOME
  ? join(process.env.CARGO_HOME, "bin")
  : join(homedir(), ".cargo", "bin");

const cargoCandidates = [
  ...pathEntries,
  cargoHome,
  "/opt/homebrew/opt/rustup/bin",
  "/usr/local/opt/rustup/bin",
  ...rustupToolchainBins(),
];

const cargoDirectory = cargoCandidates.find(containsCargo);

if (!cargoDirectory) {
  console.error(
    [
      "Onyx could not find Cargo.",
      "Install current stable Rust from https://rustup.rs, restart your terminal,",
      "then verify the installation with: cargo --version",
    ].join("\n"),
  );
  process.exit(1);
}

const tauriCli = fileURLToPath(
  new URL("../node_modules/@tauri-apps/cli/tauri.js", import.meta.url),
);

if (!existsSync(tauriCli)) {
  console.error("The Tauri CLI is not installed. Run: npm ci");
  process.exit(1);
}

const environment = {
  ...process.env,
  PATH: [cargoDirectory, ...pathEntries].join(delimiter),
};

if (
  process.platform === "darwin"
  && process.argv.slice(2).includes("build")
  && !environment.APPLE_SIGNING_IDENTITY
) {
  const identities = spawnSync(
    "/usr/bin/security",
    ["find-identity", "-v", "-p", "codesigning"],
    { encoding: "utf8" },
  );
  const available = [...(identities.stdout ?? "").matchAll(/"((?:Developer ID Application|Apple Development):[^"]+)"/g)]
    .map((match) => match[1]);
  const identity = available.find((value) => value.startsWith("Developer ID Application:"))
    ?? available.find((value) => value.startsWith("Apple Development:"));
  if (identity) {
    environment.APPLE_SIGNING_IDENTITY = identity;
    console.info(`Signing this macOS build with ${identity}`);
  }
}

const child = spawn(process.execPath, [tauriCli, ...process.argv.slice(2)], {
  env: environment,
  stdio: "inherit",
});

child.on("error", (error) => {
  console.error(`Failed to start Tauri: ${error.message}`);
  process.exit(1);
});

child.on("exit", (code) => {
  process.exit(code ?? 1);
});
