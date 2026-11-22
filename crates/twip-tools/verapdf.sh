#!/bin/sh
# Exports the corpus's top-level fixtures and the accessible fixture with `twip-tools pdf` and validates each PDF with
# veraPDF's PDF/UA-1 profile: each passes, or fails only what verapdf-known.txt says its source document causes.
# veraPDF is a pinned release: its installer is fetched into .native-deps/verapdf with its SHA-256 checked and
# installed there headless (once). Needs java (8 or later) and unzip.
set -eu
version=1.28.2
# The installer's SHA-256 is the control. It was taken once the release's .asc signature verified (gpg, 2026-11-21)
# with the key it names, fetched from keyserver.ubuntu.com (not from veraPDF itself): "Carl Wilson
# <techlead@verapdf.org>", fingerprint 13DD 102B 4DD6 9354 D12D  E5A8 3184 8632 78B1 7FE7.
sha256=d1693a5f0bf0997180f6d97e8a5568b0cf39e27eee338d439c8adb58435c1e89
url="https://software.verapdf.org/releases/1.28/verapdf-greenfield-$version-installer.zip"
deps="$(pwd)/.native-deps/verapdf"
home="$deps/$version"
digest() {
  if command -v sha256sum >/dev/null 2>&1; then sha256sum "$1" | cut -d' ' -f1; else shasum -a 256 "$1" | cut -d' ' -f1; fi
}
if [ ! -x "$home/verapdf" ]; then
  mkdir -p "$deps"
  zip="$deps/verapdf-greenfield-$version-installer.zip"
  if [ ! -f "$zip" ] || [ "$(digest "$zip")" != "$sha256" ]; then
    curl -sSfL "$url" -o "$zip"
  fi
  if [ "$(digest "$zip")" != "$sha256" ]; then
    rm -f "$zip"
    echo "veraPDF: $url isn't the pinned release (its SHA-256 differs)" >&2
    exit 1
  fi
  unpacked=$(mktemp -d)
  unzip -q "$zip" -d "$unpacked"
  cat > "$unpacked/install.xml" <<XML
<?xml version="1.0" encoding="UTF-8" standalone="no"?>
<AutomatedInstallation langpack="eng">
<com.izforge.izpack.panels.htmlhello.HTMLHelloPanel id="welcome"/>
<com.izforge.izpack.panels.target.TargetPanel id="install_dir"><installpath>$home</installpath></com.izforge.izpack.panels.target.TargetPanel>
<com.izforge.izpack.panels.packs.PacksPanel id="sdk_pack_select">
<pack index="0" name="veraPDF GUI" selected="true"/>
<pack index="1" name="veraPDF Mac and *nix Scripts" selected="true"/>
<pack index="2" name="veraPDF Batch files" selected="false"/>
<pack index="3" name="veraPDF Validation model" selected="false"/>
<pack index="4" name="veraPDF Documentation" selected="false"/>
<pack index="5" name="veraPDF Sample Plugins" selected="false"/>
</com.izforge.izpack.panels.packs.PacksPanel>
<com.izforge.izpack.panels.install.InstallPanel id="install"/>
<com.izforge.izpack.panels.finish.FinishPanel id="finish"/>
</AutomatedInstallation>
XML
  java -jar "$unpacked/verapdf-greenfield-$version/verapdf-izpack-installer-$version.jar" "$unpacked/install.xml" >/dev/null
  rm -rf "$unpacked"
fi
out=$(mktemp -d)
cargo build -q -p twip-tools
for docx in crates/twip/tests/fixtures/*.docx crates/twip/tests/fixtures/render/pdf-accessible.docx; do
  ./target/debug/twip-tools pdf "$docx" "$out/$(basename "$docx" .docx).pdf"
done
# A PDF's validation: "rules <failed rules>" (clause-test, sorted), or "none" when the report holds no validation
# result for it (a PDF veraPDF can't parse, java missing, no output): never a pass.
validate() {
  report=$("$home/verapdf" --flavour ua1 --format mrr "$1" 2>/dev/null) || true
  if ! echo "$report" | grep -q '<validationReport ' || echo "$report" | grep -q '<taskException'; then
    echo none
    return
  fi
  echo "rules $(echo "$report" | grep -o 'clause="[^"]*" testNumber="[^"]*"' \
    | sed 's/clause="\([^"]*\)" testNumber="\([^"]*\)"/\1-\2/' | sort -u | tr '\n' ' ' | sed 's/ $//')"
}
# The check itself: a broken PDF has no validation result.
broken=$(mktemp -d)/broken.pdf
printf '%%PDF-1.7\nnot a document\n' > "$broken"
if [ "$(validate "$broken")" != none ]; then
  echo "verapdf.sh: a broken PDF wasn't refused, so a pass can't be told from no result" >&2
  exit 1
fi
# Each PDF passes, or fails exactly the rules verapdf-known.txt lists for it.
known=crates/twip-tools/verapdf-known.txt
failed=0
for pdf in "$out"/*.pdf; do
  name=$(basename "$pdf")
  result=$(validate "$pdf")
  if [ "$result" = none ]; then
    echo "FAIL $name ua1: veraPDF gave no validation result" >&2
    failed=1
    continue
  fi
  rules=${result#rules }
  rules=${rules# }
  expected=$(grep -v '^#' "$known" | awk -v n="$name" '$1 == n { print $2 }' | tr ',' '\n' | sort -u | tr '\n' ' ' | sed 's/ $//')
  if [ "$rules" = "$expected" ]; then
    if [ -n "$rules" ]; then
      echo "KNOWN $name ua1: fails $rules ($(grep -v '^#' "$known" | awk -v n="$name" '$1 == n { $1 = ""; $2 = ""; sub(/^  /, ""); print }'))"
    else
      echo "PASS $name ua1"
    fi
  else
    echo "FAIL $name ua1: fails '${rules:-nothing}', expected '${expected:-nothing}' (verapdf-known.txt)" >&2
    failed=1
  fi
done
if [ "$failed" = 1 ]; then
  "$home/verapdf" --flavour ua1 --format mrr "$out"/*.pdf | grep -E 'clause=|<description>|<context>|<exceptionMessage>' >&2 || true
  exit 1
fi
echo "ok"
