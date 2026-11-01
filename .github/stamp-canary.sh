#!/bin/sh
# Makes this checkout a canary or preview, for build.yml: writes 0.0.0-canary-<short SHA> into the files release-please
# versions. 0.0.0 so a canary never satisfies a real version range (React's scheme). A hyphen before the SHA, not a dot:
# an all-digit SHA after a dot is a numeric identifier, and one with a leading zero isn't semver. Nothing is committed:
# the canary exists only in that run's checkout.
set -eu
old=$(sed -nE 's/^version = "(.+)"$/\1/p' Cargo.toml)
new=0.0.0-canary-$(git rev-parse --short=7 HEAD)
sed -i "s/^version = \"$old\"$/version = \"$new\"/" Cargo.toml
sed -i "s/\"version\": \"$old\"/\"version\": \"$new\"/" bindings/web/package.json
sed -i "s|<Version>$old</Version>|<Version>$new</Version>|" bindings/dotnet/Twip.Core/Twip.Core.csproj bindings/dotnet/Twip.WinForms/Twip.WinForms.csproj
for f in Cargo.toml bindings/web/package.json bindings/dotnet/Twip.Core/Twip.Core.csproj bindings/dotnet/Twip.WinForms/Twip.WinForms.csproj; do
  grep -qF "$new" "$f" || { echo "::error::$f still has $old"; exit 1; }
done
echo "Stamped $new (was $old)"
