#!/usr/bin/env bash
# Magellan install script — bash / zsh
# Usage: curl -sSf https://raw.githubusercontent.com/oldnordic/magellan/main/install/install.sh | bash
# Or:    bash install/install.sh [--no-service] [--no-hook]

set -euo pipefail

MAGELLAN_VERSION="4.9.1"
INSTALL_SERVICE=1
INSTALL_HOOK=1
HOOK_THRESHOLD=15

# Parse flags
for arg in "$@"; do
  case "$arg" in
    --no-service) INSTALL_SERVICE=0 ;;
    --no-hook)    INSTALL_HOOK=0 ;;
    *)            echo "Unknown flag: $arg" >&2; exit 1 ;;
  esac
done

info()  { echo "[magellan] $*"; }
warn()  { echo "[magellan] WARN: $*" >&2; }
die()   { echo "[magellan] ERROR: $*" >&2; exit 1; }

# ── Prerequisites ────────────────────────────────────────────────────────────

command -v cargo >/dev/null 2>&1 || die "Rust toolchain not found. Install from https://rustup.rs and re-run."
command -v git   >/dev/null 2>&1 || die "git not found."
command -v jq    >/dev/null 2>&1 || warn "jq not found — pre-commit blast-radius hook requires it."

# ── Install binary ──────────────────────────────────────────────────────────

info "Installing magellan ${MAGELLAN_VERSION} via cargo..."
cargo install magellan --version "${MAGELLAN_VERSION}" --locked

MAGELLAN_BIN="$(cargo install --list | grep '^magellan ' | head -1 | awk '{print $NF}' || true)"
MAGELLAN_BIN="${CARGO_HOME:-$HOME/.cargo}/bin/magellan"
[[ -x "$MAGELLAN_BIN" ]] || die "Binary not found at $MAGELLAN_BIN after install."

info "Installed: $("$MAGELLAN_BIN" --version)"

# ── Directory layout ────────────────────────────────────────────────────────

MAGELLAN_HOME="${HOME}/.magellan"
CONFIG_DIR="${HOME}/.config/magellan"
mkdir -p "$MAGELLAN_HOME" "$CONFIG_DIR"
info "Data directory: $MAGELLAN_HOME"

# ── Pre-commit blast-radius hook ────────────────────────────────────────────

HOOK_SCRIPT="${HOME}/.local/bin/pre-commit-blast-radius"
mkdir -p "${HOME}/.local/bin"

if [[ "$INSTALL_HOOK" -eq 1 ]]; then
  info "Installing pre-commit blast-radius hook → $HOOK_SCRIPT"
  cat > "$HOOK_SCRIPT" << HOOK_EOF
#!/usr/bin/env bash
# Magellan blast-radius warning hook — warn only, never blocks commit.
# Env vars: BLAST_RADIUS_THRESHOLD (default ${HOOK_THRESHOLD}), BLAST_RADIUS_MAX_SYMBOLS (default 8), BLAST_RADIUS_DEPTH (default 2)
set -euo pipefail
THRESHOLD="\${BLAST_RADIUS_THRESHOLD:-${HOOK_THRESHOLD}}"
MAX_SYMBOLS="\${BLAST_RADIUS_MAX_SYMBOLS:-8}"
DEPTH="\${BLAST_RADIUS_DEPTH:-2}"
PROJECT=\$(basename "\$(git rev-parse --show-toplevel 2>/dev/null)")
MAGELLAN_DB="\$HOME/.magellan/\$PROJECT/\$PROJECT.db"
[[ ! -f "\$MAGELLAN_DB" ]] && exit 0
STAGED=\$(git diff --cached --name-only --diff-filter=ACM 2>/dev/null \
  | grep -E '\.(rs|py|c|cpp|h|hpp|ts|js|go)$' || true)
[[ -z "\$STAGED" ]] && exit 0
WARNED=0; CHECKED=0
for file in \$STAGED; do
  [[ "\$CHECKED" -ge "\$MAX_SYMBOLS" ]] && break
  SYMBOLS=\$(magellan query --db "\$MAGELLAN_DB" --file "\$file" --output json 2>/dev/null \
    | jq -r '.data.symbols[].name' 2>/dev/null || true)
  for sym in \$SYMBOLS; do
    [[ "\$CHECKED" -ge "\$MAX_SYMBOLS" ]] && break
    CHECKED=\$((CHECKED + 1))
    RESULT=\$(magellan context impact --db "\$MAGELLAN_DB" --name "\$sym" --depth "\$DEPTH" --output json 2>/dev/null || true)
    [[ -z "\$RESULT" ]] && continue
    TOTAL=\$(echo "\$RESULT" | jq -r '.data.total_records // 0' 2>/dev/null || echo 0)
    if [[ "\$TOTAL" -gt "\$THRESHOLD" ]]; then
      [[ "\$WARNED" -eq 0 ]] && echo "" && echo "BLAST RADIUS WARNING (warn-only, commit proceeds)" && echo "──────────────────────────────────────────────────"
      echo "  ⚠  \$sym → \$TOTAL symbols affected (depth \$DEPTH)  [\$file]"
      WARNED=1
    fi
  done
done
if [[ "\$WARNED" -eq 1 ]]; then
  echo ""
  echo "Inspect: magellan context impact --db \$MAGELLAN_DB --name <symbol> --depth 3"
  echo "──────────────────────────────────────────────────"
  echo ""
fi
exit 0
HOOK_EOF
  chmod +x "$HOOK_SCRIPT"
  info "Hook installed. Enable per-project: ln -sf ~/.local/bin/pre-commit-blast-radius .git/hooks/pre-commit"
fi

# ── systemd user service (Linux only) ──────────────────────────────────────

if [[ "$INSTALL_SERVICE" -eq 1 ]] && command -v systemctl >/dev/null 2>&1; then
  SERVICE_DIR="${HOME}/.config/systemd/user"
  mkdir -p "$SERVICE_DIR"
  SERVICE_FILE="${SERVICE_DIR}/magellan.service"

  cat > "$SERVICE_FILE" << SERVICE_EOF
[Unit]
Description=Magellan code intelligence service daemon
Documentation=https://github.com/oldnordic/magellan/blob/main/MANUAL.md
After=network.target

[Service]
Type=simple
ExecStart=%h/.cargo/bin/magellan service-daemon
Restart=on-failure
RestartSec=5
KillMode=process
TimeoutStopSec=30

[Install]
WantedBy=default.target
SERVICE_EOF

  systemctl --user daemon-reload
  systemctl --user enable magellan.service
  systemctl --user start magellan.service
  info "systemd service enabled and started."
  info "Status: systemctl --user status magellan"
fi

# ── Shell env hint ──────────────────────────────────────────────────────────

SHELL_NAME="$(basename "${SHELL:-bash}")"
info ""
info "Installation complete — magellan ${MAGELLAN_VERSION}"
info ""
info "Quick start:"
info "  magellan init --path /your/project"
info "  magellan watch --root /your/project/src --db ~/.magellan/<project>/<project>.db --scan-initial"
info "  magellan orient --db ~/.magellan/<project>/<project>.db --repo /your/project"
info ""
if [[ "$SHELL_NAME" == "fish" ]]; then
  info "Fish users: see install/install.fish for fish-specific setup."
fi
info "Full manual: https://github.com/oldnordic/magellan/blob/main/MANUAL.md"
