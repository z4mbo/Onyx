#!/usr/bin/env node

import { existsSync } from "node:fs";
import { homedir } from "node:os";
import { delimiter, join } from "node:path";
import { spawn } from "node:child_process";

const executable = process.platform === "win32" ? "trunk.exe" : "trunk";
const pathEntries = (process.env.PATH ?? "").split(delimiter).filter(Boolean);
const cargoBin = process.env.CARGO_HOME
  ? join(process.env.CARGO_HOME, "bin")
  : join(homedir(), ".cargo", "bin");
const directory = [...pathEntries, cargoBin].find((entry) =>
  existsSync(join(entry, executable)),
);

if (!directory) {
  console.error(
    "Trunk is required for the Rust UI preview. Install it with: "
      + "cargo install trunk --version 0.21.14 --locked",
  );
  process.exit(1);
}

const environment = { ...process.env };
if (environment.NO_COLOR) environment.NO_COLOR = "true";

const child = spawn(join(directory, executable), process.argv.slice(2), {
  env: environment,
  stdio: "inherit",
});

child.on("error", (error) => {
  console.error(`Failed to start Trunk: ${error.message}`);
  process.exit(1);
});

child.on("exit", (code) => {
  process.exit(code ?? 1);
});
