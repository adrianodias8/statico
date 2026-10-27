#!/bin/sh
# Assemble Statico's documentation site from the `docs` template in the
# binary, the pages and settings in site/, and the reference in docs/.
#
# usage: site/assemble.sh <path-to-statico> <output-folder>
set -eu
statico=$1
out=$2
here=$(cd "$(dirname "$0")" && pwd)

rm -rf "$out"
"$statico" new "$out" --template docs --no-install
rm -f "$out"/content/docs/*.md
cp "$here/statico.json" "$out/"
cp "$here"/collections/*.json "$out/collections/"
cp "$here"/content/docs/*.md "$out/content/docs/"
python3 "$here/reference.py" "$here/../docs/REFERENCE.md" "$out/content/docs"
