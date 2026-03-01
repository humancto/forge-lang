#!/bin/bash
# Build the Programming Forge PDF book
set -e

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
DOCS_DIR="$(dirname "$SCRIPT_DIR")"
BOOK_DIR="$SCRIPT_DIR"

echo "Building Programming Forge PDF..."

# Run pandoc with tectonic as the PDF engine
pandoc "$DOCS_DIR/PROGRAMMING_FORGE.md" \
  --pdf-engine=tectonic \
  --template="$BOOK_DIR/template.tex" \
  --variable=cover-image:"$DOCS_DIR/cover.jpeg" \
  --top-level-division=chapter \
  --number-sections \
  --toc \
  --toc-depth=2 \
  --highlight-style=tango \
  --lua-filter="$BOOK_DIR/fix-links.lua" \
  -o "$BOOK_DIR/programming-forge.pdf"

echo "Done! Output: $BOOK_DIR/programming-forge.pdf"
