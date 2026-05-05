#!/usr/bin/env bash
# inject-context.sh — Extrahiert aktuelle pub-Signaturen aller Crates
# Wird vor jedem Jules-Task ausgeführt um Kontext-Erosion zu verhindern.
# Output: Markdown-Dokument mit aktuellem API-State

set -euo pipefail

CRATES=(memfuse-core memfuse-store memfuse-index memfuse-db memfuse-text memfuse-py)
TIMESTAMP=$(date -u +"%Y-%m-%dT%H:%M:%SZ")

cat <<EOF
# MemFuse — Current API State Snapshot
> Generated: ${TIMESTAMP}
> WICHTIG: Dieser Kontext ist frisch generiert. Implementiere gegen DIESE Signaturen, nicht gegen veraltete Prompt-Textbausteine.

EOF

for CRATE in "${CRATES[@]}"; do
    SRC="crates/${CRATE}/src"
    if [ ! -d "$SRC" ]; then
        echo "## ${CRATE} — (noch nicht vorhanden)"
        continue
    fi

    echo "## \`${CRATE}\`"
    echo ""
    echo "\`\`\`rust"

    # Extrahiere alle pub struct, pub fn, pub trait, pub enum Definitionen
    grep -rh \
        -e "^pub struct " \
        -e "^pub trait " \
        -e "^pub enum " \
        -e "^pub fn " \
        -e "^    pub fn " \
        -e "^pub async fn " \
        -e "^    pub async fn " \
        -e "^pub type " \
        "${SRC}" --include="*.rs" 2>/dev/null \
        | grep -v "//.*pub" \
        | sort -u \
        | head -60 \
        || echo "// (keine pub items gefunden)"

    echo "\`\`\`"
    echo ""

    # Zeige LoC-Zähler
    LOC=$(find "${SRC}" -name "*.rs" -exec wc -l {} + 2>/dev/null | tail -1 | awk '{print $1}')
    echo "> LoC: ~${LOC}"
    echo ""
done

echo "---"
echo ""
echo "## Workspace Dependencies"
echo ""
echo "\`\`\`"
cargo tree --workspace --depth 1 2>/dev/null | grep -v "^$" | head -40 || echo "(cargo tree nicht verfügbar)"
echo "\`\`\`"
