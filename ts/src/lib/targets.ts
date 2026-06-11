import path from "node:path";

export interface ProcessReportLike {
	getReport?: () => {
		header?: {
			glibcVersionRuntime?: string;
		};
	};
}

export interface ProcessLike {
	arch: string;
	platform: string;
	report?: ProcessReportLike;
}

export interface SupportedTarget {
	platform: string;
	arch: string;
	triple: string;
	label: string;
	libc?: "glibc";
}

export interface SkillyError extends Error {
	code?: string;
}

export const PACKAGE_NAME = "@xelandernt/skilly";

export const TARGETS: readonly SupportedTarget[] = [
	{
		platform: "darwin",
		arch: "arm64",
		triple: "aarch64-apple-darwin",
		label: "macOS arm64",
	},
	{
		platform: "darwin",
		arch: "x64",
		triple: "x86_64-apple-darwin",
		label: "macOS x64",
	},
	{
		platform: "linux",
		arch: "x64",
		libc: "glibc",
		triple: "x86_64-unknown-linux-gnu",
		label: "Linux x64 (glibc)",
	},
	{
		platform: "win32",
		arch: "x64",
		triple: "x86_64-pc-windows-msvc",
		label: "Windows x64",
	},
] as const;

export function supportedTargetLabels(): string[] {
	return TARGETS.map((target) => target.label);
}

export function detectLinuxLibc(
	processLike: ProcessLike = process,
): "glibc" | "musl" | "unknown" | null {
	if (processLike.platform !== "linux") {
		return null;
	}

	const report = processLike.report;
	if (!report || typeof report.getReport !== "function") {
		return "unknown";
	}

	const glibcVersionRuntime = report.getReport()?.header?.glibcVersionRuntime;
	return glibcVersionRuntime ? "glibc" : "musl";
}

export function executableName(
	target: SupportedTarget,
): "skilly" | "skilly.exe" {
	return target.platform === "win32" ? "skilly.exe" : "skilly";
}

export function vendorRelativePath(target: SupportedTarget): string {
	return path.posix.join("vendor", target.triple, executableName(target));
}

export function unsupportedPlatformError(
	processLike: ProcessLike = process,
): SkillyError {
	const libc = detectLinuxLibc(processLike);
	const details = libc
		? `${processLike.platform} ${processLike.arch} (${libc})`
		: `${processLike.platform} ${processLike.arch}`;
	const supported = supportedTargetLabels().join(", ");
	const hint =
		processLike.platform === "linux" && libc !== "glibc"
			? "Linux builds currently require glibc."
			: "No packaged binary matches this platform.";
	const error = new Error(
		`Unsupported platform for ${PACKAGE_NAME}: ${details}. Supported targets: ${supported}. ${hint}`,
	) as SkillyError;
	error.code = "ERR_UNSUPPORTED_PLATFORM";
	return error;
}

export function resolveTarget(
	processLike: ProcessLike = process,
): SupportedTarget {
	const libc = detectLinuxLibc(processLike);
	const target = TARGETS.find((candidate) => {
		if (
			candidate.platform !== processLike.platform ||
			candidate.arch !== processLike.arch
		) {
			return false;
		}
		if (candidate.platform !== "linux") {
			return true;
		}
		return candidate.libc === libc;
	});

	if (!target) {
		throw unsupportedPlatformError(processLike);
	}

	return target;
}
