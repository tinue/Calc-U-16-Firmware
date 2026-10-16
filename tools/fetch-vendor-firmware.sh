#!/bin/sh
# Downloads the SwissMicros Voyager (DM-L) factory firmware images into vendor/.
# Source: https://github.com/swissmicros/swissmicros-factorydefaults/tree/main/Voyager_Models
# These files are SwissMicros property and are git-ignored; never commit them.
set -eu

REPO=swissmicros/swissmicros-factorydefaults
DIR=Voyager_Models
DEST="$(cd "$(dirname "$0")/.." && pwd)/vendor"

mkdir -p "$DEST"
urls=$(curl -fsSL "https://api.github.com/repos/$REPO/contents/$DIR" \
    | grep -o '"download_url": *"[^"]*"' \
    | sed 's/.*"\(https[^"]*\)"/\1/')
[ -n "$urls" ] || { echo "no files found (GitHub API rate limit?)" >&2; exit 1; }

for url in $urls; do
    name=$(basename "$url")
    echo "fetching $name"
    curl -fsSL -o "$DEST/$name" "$url"
done
echo "done: $(ls "$DEST" | wc -l | tr -d ' ') files in $DEST"
