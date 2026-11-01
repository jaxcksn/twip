#!/bin/sh
# Makes this checkout a canary, for build.yml: writes x.y.z-canary-<short SHA> (0.1.0-alpha.1 -> 0.1.0-canary-1a2b3c4)
# into the files release-please versions, and names the npm package @<owner>/twip-web, the only scope GitHub's npm
# registry takes. A hyphen before the SHA, not a dot: an all-digit SHA after a dot is a numeric identifier, and one with
# a leading zero isn't semver. Nothing is committed: the canary exists only in that run's checkout.
set -eu
old=$(sed -nE 's/^version = "(.+)"$/\1/p' Cargo.toml)
new=$(echo "$old" | sed -nE 's/^([0-9]+\.[0-9]+\.[0-9]+).*/\1/p')-canary-$(git rev-parse --short=7 HEAD)
echo "$new" | grep -Eq '^[0-9]+\.[0-9]+\.[0-9]+-canary-[0-9a-f]{7}$' || { echo "::error::'$new' is not x.y.z-canary-<short SHA>"; exit 1; }
sed -i "s/^version = \"$old\"$/version = \"$new\"/" Cargo.toml
sed -i "s/\"version\": \"$old\"/\"version\": \"$new\"/" bindings/web/package.json
sed -i "s|<Version>$old</Version>|<Version>$new</Version>|" bindings/dotnet/Twip.Core/Twip.Core.csproj bindings/dotnet/Twip.WinForms/Twip.WinForms.csproj
for f in Cargo.toml bindings/web/package.json bindings/dotnet/Twip.Core/Twip.Core.csproj bindings/dotnet/Twip.WinForms/Twip.WinForms.csproj; do
  grep -qF "$new" "$f" || { echo "::error::$f still has $old"; exit 1; }
done
(cd bindings/web && npm pkg set name="@$GITHUB_REPOSITORY_OWNER/twip-web")
echo "Stamped $new (was $old)"
