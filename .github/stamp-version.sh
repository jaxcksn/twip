#!/bin/sh
# Writes a prerelease version (0.1.1-pre.1) into the files release-please versions, for a manual prerelease publish
# from release.yml. Nothing is committed: the version exists only in that run's checkout.
set -eu
new=$1
echo "$new" | grep -Eq '^[0-9]+\.[0-9]+\.[0-9]+-pre\.[0-9]+$' || { echo "::error::'$new' is not x.y.z-pre.N"; exit 1; }
old=$(sed -nE 's/^version = "(.+)"$/\1/p' Cargo.toml)
sed -i "s/^version = \"$old\"$/version = \"$new\"/" Cargo.toml
sed -i "s/\"version\": \"$old\"/\"version\": \"$new\"/" bindings/web/package.json
sed -i "s|<Version>$old</Version>|<Version>$new</Version>|" bindings/dotnet/Twip/Twip.csproj
for f in Cargo.toml bindings/web/package.json bindings/dotnet/Twip/Twip.csproj; do
  grep -qF "$new" "$f" || { echo "::error::$f still has $old"; exit 1; }
done
echo "Stamped $new (was $old)"
