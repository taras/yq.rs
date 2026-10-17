set -eu
platform="$1"
fixture=$(mktemp -d)
version=$(node -p "JSON.parse(require('fs').readFileSync('package.json')).version")
export YQ_TEST_NATIVE_ADDON_PACKAGE="/work/target/release-artifacts/yq.rs-$platform-napi-$version.tgz"
npm install --prefix "$fixture" --offline --ignore-scripts --no-audit --no-fund --no-package-lock \
  "/work/target/release-artifacts/yq.rs-$version.tgz" \
  "/work/target/release-artifacts/yq.rs-$platform-$version.tgz" "$YQ_TEST_NATIVE_ADDON_PACKAGE"
cp js/smoke.mjs "$fixture/smoke.mjs"
(cd "$fixture" && node smoke.mjs && YQ_FORCE_WASM=1 node smoke.mjs)
printf 'name: Ada\n' | node "$fixture/node_modules/yq.rs/dist/cli.mjs" .name | grep -qx Ada
mv "$fixture/node_modules/yq.rs/dist/inline.js" "$fixture/inline.hidden"
printf 'name: Ada\n' | node "$fixture/node_modules/yq.rs/dist/cli.mjs" .name | grep -qx Ada
