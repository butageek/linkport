#!/usr/bin/env bash
# Package the Windows release into a shareable zip:
#   dist/linkport-<version>-win64.zip
#   └─ linkport-<version>-win64/ { linkport.exe, linkport-cli.exe,
#      QUICK-START.txt, LICENSE }
#
# Runs the full build first (frontend + cross-compile) so the zip is
# always fresh. Recipients follow QUICK-START.txt; expect SmartScreen to
# warn on the unsigned exes ("More info" > "Run anyway").
set -euo pipefail
cd "$(dirname "$0")/.."

VERSION=$(grep -m1 '^version' Cargo.toml | sed 's/version = "\(.*\)"/\1/')
TARGET_DIR=target/x86_64-pc-windows-gnu/release
STAGE="dist/linkport-$VERSION-win64"

./scripts/build.sh

rm -rf "$STAGE"
mkdir -p "$STAGE"
cp "$TARGET_DIR/linkport.exe" "$TARGET_DIR/linkport-cli.exe" "$STAGE/"
cp resources/quick-start.txt "$STAGE/QUICK-START.txt"
cp LICENSE "$STAGE/"

# No `zip` in a default WSL install; python3's zipfile is always there.
# (Concatenate the name — with_suffix would mangle the dotted version.)
rm -f "$STAGE.zip"
python3 - "$STAGE" <<'EOF'
import pathlib, sys, zipfile

stage = pathlib.Path(sys.argv[1])
out = stage.parent / f"{stage.name}.zip"
with zipfile.ZipFile(out, "w", zipfile.ZIP_DEFLATED) as zf:
    for p in sorted(stage.rglob("*")):
        if p.is_file():
            zf.write(p, p.relative_to(stage.parent))
EOF
rm -rf "$STAGE"

echo
echo "Done: dist/linkport-$VERSION-win64.zip — attach it to a GitHub Release"
echo "(pushing a v* tag lets .github/workflows/release.yml do this for you)."
