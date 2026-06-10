import fs from "node:fs";
import path from "node:path";
import { spawn, type ChildProcess, type SpawnOptions } from "node:child_process";
import {
  PACKAGE_NAME,
  type ProcessLike as PlatformProcessLike,
  type SkillyError,
  resolveTarget,
  vendorRelativePath
} from "./targets";

export interface LauncherProcessLike extends PlatformProcessLike {
  env: NodeJS.ProcessEnv;
  exit: (code?: number) => never | void;
  kill: (pid: number, signal: NodeJS.Signals) => boolean | void;
  pid: number;
  stderr?: {
    write: (chunk: string) => unknown;
  };
}

export type SpawnLike = (
  command: string,
  args: readonly string[],
  options: SpawnOptions
) => ChildProcess;

export interface LaunchOptions {
  processLike?: LauncherProcessLike;
  rootDir?: string;
  spawnImpl?: SpawnLike;
}

export function resolveBinaryPath(options: Pick<LaunchOptions, "processLike" | "rootDir"> = {}): string {
  const rootDir = options.rootDir ?? path.resolve(__dirname, "..", "..", "..");
  const processLike = options.processLike ?? process;
  const target = resolveTarget(processLike);
  return path.join(rootDir, vendorRelativePath(target));
}

export function ensureBinaryExists(binaryPath: string): void {
  if (fs.existsSync(binaryPath)) {
    return;
  }

  const error = new Error(
    `No packaged skilly binary was found at ${binaryPath}. Reinstall ${PACKAGE_NAME} or rebuild the npm package with staged native binaries.`
  ) as SkillyError;
  error.code = "ERR_MISSING_BINARY";
  throw error;
}

export function launch(
  argv: readonly string[],
  options: LaunchOptions = {}
): { child: ChildProcess; binaryPath: string } {
  const processLike = options.processLike ?? process;
  const spawnImpl = options.spawnImpl ?? spawn;
  const binaryPath = resolveBinaryPath(options);
  ensureBinaryExists(binaryPath);

  const child = spawnImpl(binaryPath, argv, {
    env: processLike.env,
    stdio: "inherit",
    windowsHide: false
  });
  return { child, binaryPath };
}

function writeError(processLike: LauncherProcessLike, message: string): void {
  if (processLike.stderr && typeof processLike.stderr.write === "function") {
    processLike.stderr.write(`${message}\n`);
    return;
  }
  console.error(message);
}

export function run(argv: readonly string[], options: LaunchOptions = {}): ChildProcess {
  const processLike = options.processLike ?? process;
  const { child, binaryPath } = launch(argv, options);

  child.once("error", (error: Error) => {
    writeError(processLike, `Failed to launch skilly from ${binaryPath}: ${error.message}`);
    processLike.exit(1);
  });

  child.once("close", (code: number | null, signal: NodeJS.Signals | null) => {
    if (signal) {
      processLike.kill(processLike.pid, signal);
      return;
    }
    processLike.exit(code ?? 1);
  });

  return child;
}
