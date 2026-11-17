"""Builds the Obsidian plugin: main.js with the WebAssembly core inlined,
manifest.json and styles.css.

    cargo build -p dedalo-wasm --target wasm32-unknown-unknown --profile wasm
    python integrations/obsidian/build.py                  # into integrations/obsidian/dist
    python integrations/obsidian/build.py --zip dedalo-obsidian.zip
    python integrations/obsidian/build.py --install PATH/TO/VAULT

Obsidian loads a plugin from `main.js`, `manifest.json` and `styles.css`
only, so the core travels inside `main.js` as base64. `--install` copies the
three files into the vault's `.obsidian/plugins/dedalo/` and keeps the
plugin's settings (`data.json`).
"""

import argparse
import base64
import json
import pathlib
import shutil
import sys
import zipfile

HERE = pathlib.Path(__file__).resolve().parent
ROOT = HERE.parent.parent
PLACEHOLDER = '"__DEDALO_WASM__"'
FILES = ["main.js", "manifest.json", "styles.css"]


def build(wasm: pathlib.Path, out: pathlib.Path) -> None:
    source = (HERE / "plugin.js").read_text(encoding="utf-8")
    if source.count(PLACEHOLDER) != 1:
        sys.exit("plugin.js must hold the WebAssembly placeholder exactly once")
    encoded = base64.b64encode(wasm.read_bytes()).decode("ascii")
    out.mkdir(parents=True, exist_ok=True)
    (out / "main.js").write_text(source.replace(PLACEHOLDER, json.dumps(encoded)), encoding="utf-8", newline="\n")
    for name in ["manifest.json", "styles.css"]:
        shutil.copyfile(HERE / name, out / name)


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--wasm", type=pathlib.Path, help="the core module; default: the wasm build, else web/dedalo.wasm")
    parser.add_argument("--out", type=pathlib.Path, default=HERE / "dist")
    parser.add_argument("--zip", type=pathlib.Path, help="also write a zip holding a `dedalo` folder")
    parser.add_argument("--install", type=pathlib.Path, metavar="VAULT", help="also copy the plugin into this vault")
    args = parser.parse_args()

    wasm = args.wasm
    if wasm is None:
        for candidate in [ROOT / "target/wasm32-unknown-unknown/wasm/dedalo_wasm.wasm", ROOT / "web/dedalo.wasm"]:
            if candidate.is_file():
                wasm = candidate
                break
    if wasm is None or not wasm.is_file():
        sys.exit("no core module; build it with: cargo build -p dedalo-wasm --target wasm32-unknown-unknown --profile wasm")

    build(wasm, args.out)
    print(f"built {args.out} from {wasm}")

    if args.zip:
        with zipfile.ZipFile(args.zip, "w", zipfile.ZIP_DEFLATED) as archive:
            for name in FILES:
                archive.write(args.out / name, f"dedalo/{name}")
        print(f"wrote {args.zip}")

    if args.install:
        if not (args.install / ".obsidian").is_dir():
            sys.exit(f"{args.install} is not an Obsidian vault: it has no .obsidian folder")
        target = args.install / ".obsidian" / "plugins" / "dedalo"
        target.mkdir(parents=True, exist_ok=True)
        for name in FILES:
            shutil.copyfile(args.out / name, target / name)
        print(f"installed into {target}; enable Dedalo under Settings, Community plugins")


if __name__ == "__main__":
    main()
