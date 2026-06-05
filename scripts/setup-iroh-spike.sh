#!/usr/bin/env bash
#
# setup-iroh-spike.sh — install dependencies for the Alt.Drive iroh spike
# (Phase 0, Spike 2 — iroh-blobs hello-world between two nodes).
#
# Targets Debian/Ubuntu. Idempotent: safe to re-run.
# Requires: passwordless sudo (or run interactively and enter password when
# prompted by apt).
#
# What this installs:
#   - apt: build-essential, pkg-config, libssl-dev, curl, ca-certificates, git
#   - rustup-managed Rust toolchain (stable, minimal profile) — only if cargo
#     is not already on PATH
#
# What this does NOT do (manual steps printed at the end):
#   - AWS Security Group changes (allow UDP 2112 between the two hosts)
#   - GitHub SSH key setup / repo clone
#   - Building or running iroh code (the spike crate doesn't exist yet)

set -euo pipefail

if ! command -v apt-get >/dev/null 2>&1; then
    echo "ERROR: this script targets Debian/Ubuntu (apt-get not found)." >&2
    exit 1
fi

echo "==> Installing system packages (apt)..."
sudo apt-get update -qq
sudo apt-get install -y \
    build-essential \
    pkg-config \
    libssl-dev \
    curl \
    ca-certificates \
    git

if ! command -v cargo >/dev/null 2>&1; then
    echo "==> Installing Rust toolchain via rustup..."
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \
        | sh -s -- -y --default-toolchain stable --profile minimal
else
    echo "==> Rust already installed; skipping rustup."
fi

# Make cargo available in this shell for the verification step below.
if [ -f "$HOME/.cargo/env" ]; then
    # shellcheck source=/dev/null
    source "$HOME/.cargo/env"
fi

echo
echo "==> Versions:"
rustc --version
cargo --version
cc --version | head -n1

cat <<'EOF'

==> Setup complete on this host.

NEXT — manual steps that cannot be done from this script:

  1. AWS Security Group on EACH instance:
       Inbound rule: UDP 2112, source = the other instance's SG or /32.
       (iroh will bind to UDP 2112 explicitly in the spike crate.)

  2. New shell or `source $HOME/.cargo/env` to put cargo on PATH.

  3. Clone the repo if you haven't (HTTPS works without an SSH key):
       git clone https://github.com/AltID/alt.drive.git

  4. The iroh spike crate (crates/altdrive-spike-iroh/) does not exist
     yet. It will be added in a follow-up commit; pull again after that.

EOF
