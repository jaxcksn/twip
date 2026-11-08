#!/bin/sh
# Opens a protected PDF with keywords, written by `twip-tools pdf`, in qpdf and poppler as a reader would: it must
# refuse to open without the password, use AES-256 (revision 6), check clean with it, and list the keywords.
set -eu
out=$(mktemp -d)
pdf="$out/locked.pdf"
password='s3cret ü'
cargo run -q -p twip-tools -- pdf crates/twip/tests/fixtures/text-basics.docx "$pdf" --password "$password" --keywords "2026-CV-1042,notice,civil"
if qpdf --check "$pdf" >/dev/null 2>&1; then
  echo "the PDF opens without its password" >&2
  exit 1
fi
qpdf --password="$password" --check "$pdf"
encryption=$(qpdf --password="$password" --show-encryption "$pdf")
echo "$encryption" | grep -q "R = 6" || { echo "not revision 6: $encryption" >&2; exit 1; }
echo "$encryption" | grep -q "AESv3" || { echo "not AES-256: $encryption" >&2; exit 1; }
pdfinfo -upw "$password" "$pdf" | grep -q "^Keywords: *2026-CV-1042, notice, civil$" || { pdfinfo -upw "$password" "$pdf" >&2; exit 1; }
pdftotext -upw "$password" "$pdf" - | grep -q "[A-Za-z]" || { echo "no text extracts" >&2; exit 1; }
echo "ok"
