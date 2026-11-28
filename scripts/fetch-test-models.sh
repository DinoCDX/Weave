#!/usr/bin/env sh
set -eu

cd "$(dirname "$0")/.."

DEST="assets/external"
BASE="https://raw.githubusercontent.com/alecjacobson/common-3d-test-models/master/data"

mkdir -p "$DEST"

fetch() {
    name="$1"
    if [ -s "$DEST/$name" ]; then
        echo "have $DEST/$name"
        return
    fi
    echo "downloading $name"
    curl -fL --retry 3 -o "$DEST/$name.part" "$BASE/$name"
    mv "$DEST/$name.part" "$DEST/$name"
}

fetch armadillo.obj

echo "test models are in $DEST (not committed; see docs/models.md for sources and licences)"
