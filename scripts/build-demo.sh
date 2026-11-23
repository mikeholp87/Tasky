#!/usr/bin/env bash
# Builds the interactive website demo into docs/demo: the real Tasky interface
# running on the synthetic data from src/dev/preview.ts, with no backend.
set -euo pipefail
cd "$(dirname "$0")/.."

VITE_DEMO=1 npx vite build --base ./ --outDir docs/demo --emptyOutDir

# A bar above the app says what this is and links back to the site.
banner='<div id="demo-bar"><span>Interactive demo with sample data. It cannot see your computer.</span><a href="../">Back to the overview</a><a href="../#install">Install on Linux</a></div>'
style='<style>#demo-bar{position:fixed;top:0;left:0;right:0;z-index:99999;height:34px;display:flex;gap:16px;align-items:center;justify-content:center;padding:0 12px;background:#fff4d6;color:#5c4200;font:13px system-ui,sans-serif;white-space:nowrap;overflow:hidden}#demo-bar a{color:#2b3590;font-weight:600}body{padding-top:34px}#root{height:calc(100vh - 34px)}@media(prefers-color-scheme:dark){#demo-bar{background:#2d2610;color:#f4d98a}#demo-bar a{color:#a3abff}}</style>'
sed -i -e "s|</head>|$style</head>|" -e "s|<div id=\"root\">|$banner<div id=\"root\">|" docs/demo/index.html
grep -q 'id="demo-bar"' docs/demo/index.html
