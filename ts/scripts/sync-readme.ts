#!/usr/bin/env node

import fs from "node:fs";
import path from "node:path";

const packageRoot = path.resolve(__dirname, "..", "..");
const repoRoot = path.resolve(packageRoot, "..");
const sourceReadme = path.join(repoRoot, "README.md");
const destinationReadme = path.join(packageRoot, "README.md");

fs.rmSync(destinationReadme, { force: true });
fs.copyFileSync(sourceReadme, destinationReadme);
