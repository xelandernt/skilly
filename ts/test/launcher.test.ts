import assert from "node:assert/strict";
import { EventEmitter } from "node:events";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import test from "node:test";
import { launch, resolveBinaryPath, run, type LauncherProcessLike } from "../src/lib/launcher";

function makeProcessLike(
  overrides: Partial<LauncherProcessLike> = {}
): LauncherProcessLike & { exitCalls: number[]; killCalls: Array<{ pid: number; signal: NodeJS.Signals }>; stderrWrites: string[] } {
  const stderrWrites: string[] = [];
  return {
    arch: "arm64",
    env: {},
    exit(code?: number) {
      this.exitCalls.push(code ?? 0);
    },
    exitCalls: [],
    kill(pid: number, signal: NodeJS.Signals) {
      this.killCalls.push({ pid, signal });
      return true;
    },
    killCalls: [],
    pid: 4242,
    platform: "darwin",
    report: {
      getReport() {
        return { header: {} };
      }
    },
    stderr: {
      write(chunk: string) {
        stderrWrites.push(chunk);
      }
    },
    stderrWrites,
    ...overrides
  };
}

function makePackagedBinary(rootDir: string, processLike: LauncherProcessLike): string {
  const binaryPath = resolveBinaryPath({ processLike, rootDir });
  fs.mkdirSync(path.dirname(binaryPath), { recursive: true });
  fs.writeFileSync(binaryPath, "#!/bin/sh\nexit 0\n", { encoding: "utf-8" });
  fs.chmodSync(binaryPath, 0o755);
  return binaryPath;
}

test("launch resolves the packaged binary and inherits stdio", () => {
  const rootDir = fs.mkdtempSync(path.join(os.tmpdir(), "skilly-launch-"));
  const processLike = makeProcessLike();
  makePackagedBinary(rootDir, processLike);
  const spawnCalls: Array<{
    command: string;
    args: readonly string[];
    options: { stdio?: unknown; windowsHide?: unknown };
  }> = [];
  const child = new EventEmitter();

  const result = launch(["--help"], {
    processLike,
    rootDir,
    spawnImpl(command, args, options) {
      spawnCalls.push({ command, args, options });
      return child as never;
    }
  });

  assert.equal(result.child, child);
  const [spawnCall] = spawnCalls;
  if (spawnCall === undefined) {
    throw new Error("expected spawn to be called");
  }
  assert.equal(spawnCall.command, resolveBinaryPath({ processLike, rootDir }));
  assert.deepEqual(spawnCall.args, ["--help"]);
  assert.equal(spawnCall.options.stdio, "inherit");
  assert.equal(spawnCall.options.windowsHide, false);
});

test("run exits with the child exit code", () => {
  const rootDir = fs.mkdtempSync(path.join(os.tmpdir(), "skilly-run-exit-"));
  const processLike = makeProcessLike();
  makePackagedBinary(rootDir, processLike);
  const child = new EventEmitter();

  run(["list"], {
    processLike,
    rootDir,
    spawnImpl() {
      return child as never;
    }
  });

  child.emit("close", 7, null);
  assert.deepEqual(processLike.exitCalls, [7]);
});

test("run re-raises child signals on the current process", () => {
  const rootDir = fs.mkdtempSync(path.join(os.tmpdir(), "skilly-run-signal-"));
  const processLike = makeProcessLike();
  makePackagedBinary(rootDir, processLike);
  const child = new EventEmitter();

  run(["list"], {
    processLike,
    rootDir,
    spawnImpl() {
      return child as never;
    }
  });

  child.emit("close", null, "SIGINT");
  assert.deepEqual(processLike.killCalls, [{ pid: 4242, signal: "SIGINT" }]);
  assert.deepEqual(processLike.exitCalls, []);
});

test("run reports launcher errors and exits with code 1", () => {
  const rootDir = fs.mkdtempSync(path.join(os.tmpdir(), "skilly-run-error-"));
  const processLike = makeProcessLike();
  makePackagedBinary(rootDir, processLike);
  const child = new EventEmitter();

  run(["list"], {
    processLike,
    rootDir,
    spawnImpl() {
      return child as never;
    }
  });

  child.emit("error", new Error("spawn failed"));
  assert.deepEqual(processLike.exitCalls, [1]);
  assert.match(processLike.stderrWrites.join(""), /Failed to launch skilly/);
});
