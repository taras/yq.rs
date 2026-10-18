set -eu
fixture=$(mktemp -d)
version=$(node -p "JSON.parse(require('fs').readFileSync('package.json')).version")
npm install --prefix "$fixture" --offline --ignore-scripts --no-audit --no-fund --no-package-lock \
  "/work/target/release-artifacts/yq.rs-$version.tgz"
cp js/smoke.mjs "$fixture/smoke.mjs"
(cd "$fixture" && node smoke.mjs && YQ_FORCE_WASM=1 node smoke.mjs)
printf 'name: Ada\n' | node "$fixture/node_modules/yq.rs/dist/cli.mjs" .name | grep -qx Ada
mv "$fixture/node_modules/yq.rs/dist/inline.js" "$fixture/inline.hidden"
printf 'name: Ada\n' | node "$fixture/node_modules/yq.rs/dist/cli.mjs" .name | grep -qx Ada
