#!/usr/bin/env bash
# Builds the interactive website demo into docs/embed: the real Tasky interface
# running on the synthetic data from src/dev/preview.ts, with no backend.
set -euo pipefail
cd "$(dirname "$0")/.."

VITE_DEMO=1 npx vite build --base ./ --outDir docs/embed --emptyOutDir

# The demo runs inside the landing page hero, so keep search engines on that page.
sed -i 's|<meta charset="UTF-8" />|&<meta name="robots" content="noindex" />|' docs/embed/index.html
grep -q 'name="robots"' docs/embed/index.html
