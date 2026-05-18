from pathlib import Path


def make_venv(
    root: Path,
    *,
    site_packages_relative: Path = Path("lib/python3.13/site-packages"),
) -> tuple[Path, Path]:
    venv_path = root / ".venv"
    site_packages = venv_path / site_packages_relative
    site_packages.mkdir(parents=True)
    return venv_path, site_packages.resolve()


def write_distribution(
    *,
    site_packages: Path,
    package_name: str,
    package_version: str,
    record_rows: list[str],
) -> None:
    dist_info = site_packages / f"{package_name}-{package_version}.dist-info"
    dist_info.mkdir()
    (dist_info / "METADATA").write_text(
        "\n".join(
            [
                "Metadata-Version: 2.4",
                f"Name: {package_name}",
                f"Version: {package_version}",
                "",
            ]
        ),
        encoding="utf-8",
    )
    (dist_info / "RECORD").write_text("\n".join(record_rows), encoding="utf-8")


def write_skill(path: Path, content: str) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(content, encoding="utf-8")
