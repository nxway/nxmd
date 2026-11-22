"""Collect redistribution texts from the locked Windows application dependencies."""
import json
import re
import shutil
import subprocess
import sys
from pathlib import Path

target = "x86_64-pc-windows-msvc"
output = Path(sys.argv[1])
output.mkdir(parents=True, exist_ok=True)


def cargo(*args):
    return subprocess.check_output(["cargo", *args], encoding="utf-8")


metadata = json.loads(cargo("metadata", "--locked", "--format-version", "1", "--filter-platform", target))
packages = {(p["name"], p["version"]): p for p in metadata["packages"]}
tree = cargo("tree", "--locked", "-p", "nxmd", "--target", target, "-e", "normal",
             "--prefix", "none", "--format", "{p}")
compiled = sorted(set(re.findall(r"^(\S+) v(\S+)", tree, re.MULTILINE)))
roots = {p["name"]: Path(p["manifest_path"]).parent for p in packages.values()}
apache = roots["gpui-pre"] / "LICENSE-APACHE"


# These crates omit their license files from the published archive.
supplements = {"seahash": "seahash-MIT.txt", "taffy": "taffy-MIT.txt",
               "clipboard-win": "BSL-1.0.txt"}
license_name = re.compile(r"^(licen[sc]e|notice|copying|copyright|unlicense|authors|patents|ofl)([_. -].*)?$", re.I)
texts = {}
index = []
for name, version in compiled:
    if name == "nxmd":
        continue
    pkg = packages[name, version]
    root = Path(pkg["manifest_path"]).parent
    files = sorted(f for f in root.rglob("*") if f.is_file() and license_name.fullmatch(f.name))
    if pkg["license_file"]:
        files.append(root / pkg["license_file"])
    if name in supplements:
        files.append(Path("licenses") / supplements[name])
    if not files:
        # Choose Apache for the dual-licensed crates that omit both texts.
        choices = (pkg["license"] or "").replace("/", " OR ").split(" OR ")
        if "Apache-2.0" not in choices or " AND " in pkg["license"]:
            raise ValueError(f"Missing redistribution text for {name} {version}")
        files = [apache]
    refs = []
    for file in files:
        value = file.read_text(encoding="utf-8-sig").strip()
        if not value:
            raise ValueError(f"Empty license file {file}")
        number = texts.setdefault(value, len(texts) + 1)
        refs.append(f"{file.name} [{number}]")
    index.extend([f"{name} {version}    {pkg['license'] or 'see license file'}",
                  "    " + ", ".join(pkg["authors"]), "    " + ", ".join(refs)])
with (output / "DEPENDENCY-NOTICES.txt").open("w", encoding="utf-8", newline="\n") as out:
    out.write("Windows x64 dependencies and their redistribution texts\n\n")
    out.write("\n".join(index))
    for value, number in texts.items():
        out.write(f"\n\n[{number}]\n{value}\n")

sysroot = subprocess.check_output(["rustc", "--print", "sysroot"], encoding="utf-8").strip()
rust_docs = Path(sysroot) / "share/doc/rust"
shutil.copyfile(rust_docs / "licenses/Apache-2.0.txt", output / "LICENSE-APACHE-2.0.txt")
shutil.copyfile(rust_docs / "COPYRIGHT-library.html", output / "RUST-STANDARD-LIBRARY.html")
