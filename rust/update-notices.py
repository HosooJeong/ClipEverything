"""Refresh the in-app license notices for the locked Windows x64 build."""

import json
from pathlib import Path
import re
import subprocess


def main():
    root = Path(__file__).resolve().parent
    metadata = json.loads(subprocess.check_output(
        ["cargo", "metadata", "--offline", "--locked", "--format-version", "1",
         "--filter-platform", "x86_64-pc-windows-msvc"],
        cwd=root,
        encoding="utf-8",
    ))
    packages = sorted(
        (p for p in metadata["packages"] if p["source"] is not None),
        key=lambda p: (p["name"], p["version"]),
    )
    index = []
    texts = {}
    order = []
    for package in packages:
        source = Path(package["manifest_path"]).parent
        files = sorted(
            p for p in source.iterdir()
            if p.is_file() and p.name.upper().startswith(
                ("LICENSE", "LICENCE", "COPYING", "NOTICE", "COPYRIGHT", "UNLICENSE", "THIRD_PARTY")
            )
        )
        if package["license_file"]:
            files = sorted(set(files + [source / package["license_file"]]))
        if not files:
            raise RuntimeError(f"License text missing for {package['name']} {package['version']}")
        entries = []
        for file in files:
            lines = file.read_text(encoding="utf-8-sig").splitlines()
            body = "\n".join(line.rstrip() for line in lines).strip()
            # Share identical text even when crate authors wrap the lines differently.
            key = re.sub(r"\s+", " ", body)
            if key not in texts:
                texts[key] = len(order) + 1
                order.append(body)
            entries.append(f"{file.name} [{texts[key]}]")
        index.append(
            f"{package['name']} {package['version']} — {package['license'] or package['license_file']}\n"
            f"  {', '.join(entries)}"
        )

    header = (
        "Third-party notices for ClipEverything\n"
        "Windows x64 Cargo build and runtime dependencies, resolved from Cargo.lock.\n"
        "The license expressions below are those declared by the respective packages.\n"
        "Each numbered text is reproduced once and referenced by the package index.\n"
        "Refresh with: python update-notices.py\n\n"
        "Bundled SQLite — Public domain dedication by the SQLite authors.\n"
        "https://www.sqlite.org/copyright.html\n\n"
        "Rust standard library — MIT OR Apache-2.0 unless otherwise specified.\n"
        "Rust Project Developers and the respective component copyright holders.\n"
        "https://github.com/rust-lang/rust/blob/master/COPYRIGHT\n\n"
        "Package index\n\n"
    )
    notices = "\n\n".join(f"[{number}]\n{body}" for number, body in enumerate(order, 1))
    output = root / "THIRD-PARTY-NOTICES.txt"
    output.write_text(header + "\n\n".join(index) + "\n\nLicense texts\n\n" + notices + "\n",
                      encoding="utf-8", newline="\n")
    print(f"{len(packages)} packages, {len(order)} license texts, {output.stat().st_size} bytes")


if __name__ == "__main__":
    main()
