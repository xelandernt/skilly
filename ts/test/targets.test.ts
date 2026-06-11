import assert from "node:assert/strict";
import test from "node:test";
import {
	detectLinuxLibc,
	resolveTarget,
	unsupportedPlatformError,
	vendorRelativePath,
} from "../src/lib/targets";

test("resolveTarget maps macOS arm64 to the packaged triple", () => {
	const target = resolveTarget({
		arch: "arm64",
		platform: "darwin",
		report: {
			getReport() {
				return { header: {} };
			},
		},
	});

	assert.equal(target.triple, "aarch64-apple-darwin");
	assert.equal(
		vendorRelativePath(target),
		"vendor/aarch64-apple-darwin/skilly",
	);
});

test("detectLinuxLibc recognizes glibc from the Node process report", () => {
	const libc = detectLinuxLibc({
		platform: "linux",
		arch: "x64",
		report: {
			getReport() {
				return { header: { glibcVersionRuntime: "2.39" } };
			},
		},
	});

	assert.equal(libc, "glibc");
});

test("resolveTarget rejects unsupported linux musl builds with an actionable error", () => {
	assert.throws(
		() =>
			resolveTarget({
				arch: "x64",
				platform: "linux",
				report: {
					getReport() {
						return { header: {} };
					},
				},
			}),
		/Linux builds currently require glibc/,
	);
});

test("unsupportedPlatformError includes the scoped package name and support matrix", () => {
	const error = unsupportedPlatformError({
		arch: "arm64",
		platform: "win32",
		report: {
			getReport() {
				return { header: {} };
			},
		},
	});

	assert.equal(error.code, "ERR_UNSUPPORTED_PLATFORM");
	assert.match(error.message, /@xelandernt\/skilly/);
	assert.match(error.message, /Supported targets:/);
});
