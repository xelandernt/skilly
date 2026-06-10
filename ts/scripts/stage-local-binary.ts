#!/usr/bin/env node

import fs from "node:fs";
import path from "node:path";
import { executableName, resolveTarget, vendorRelativePath } from "../src/lib/targets";

const packageRoot = path.resolve(__dirname, "..", "..");
const repoRoot = path.resolve(packageRoot, "..");
const target = resolveTarget(process);
const sourceBinary =
  process.argv[2] ??
  path.join(repoRoot, "target", "release", executableName(target));
const destinationBinary = path.join(packageRoot, vendorRelativePath(target));

fs.rmSync(path.join(packageRoot, "vendor"), { recursive: true, force: true });
fs.mkdirSync(path.dirname(destinationBinary), { recursive: true });
fs.copyFileSync(sourceBinary, destinationBinary);

if (process.platform !== "win32") {
  fs.chmodSync(destinationBinary, 0o755);
}

console.log(`Staged ${sourceBinary} -> ${destinationBinary}`);
