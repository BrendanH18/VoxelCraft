#!/usr/bin/env bash
# Run the hand-off checks on a Boat sandbox instead of this Mac.
#
#   scripts/boat-check.sh            cargo fmt here, then clippy + tests in Boat
#   scripts/boat-check.sh --full     also the --no-default-features clippy + tests
#   scripts/boat-check.sh -- CMD...  run CMD in the pushed checkout instead
#   scripts/boat-check.sh --save     after a passing run, save the sandbox as the
#                                    warm snapshot later runs start from
#
# `boat run` pushes the working tree (tracked and untracked, non-ignored files)
# to a new sandbox, runs the command and deletes the sandbox. Runs start from
# the saved snapshot voxelcraft-warm (system packages, toolchain and a built
# target directory) when it exists, else from a bare machine. Overrides:
# VC_BOAT_TYPE (default large: the release test build needs 16 GB) and
# VC_BOAT_SNAPSHOT. See docs/development.md#boat-checks.
main() {
set -euo pipefail
export BOAT_NO_UPDATE=1
type=${VC_BOAT_TYPE:-large}
if [ -z "${VC_BOAT_TYPE:-}" ] && [ "$(boat limits --json 2>/dev/null | jq -r '[.machineTypes[]? | select(.type == "large") | .available | tostring][0] // "true"')" = false ]; then
    type=default   # the free trial can't start large
    echo "boat-check: this account can't start large sandboxes yet; using default (8 GB), where the full release test build runs out of memory" >&2
fi
snapshot=${VC_BOAT_SNAPSHOT:-voxelcraft-warm}
cmd="" full=0 save=0
while [ $# -gt 0 ]; do
    case "$1" in
        --full) full=1 ;;
        --save) save=1 ;;
        --) shift; if [ $# -eq 1 ]; then cmd=$1; else cmd=$(printf '%q ' "$@"); fi; break ;;
        -h|--help) sed -n '2,15p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
        *) echo "boat-check: unknown option $1 (try --help)" >&2; exit 2 ;;
    esac
    shift
done
command -v boat >/dev/null || { echo "boat-check: boat is not installed" >&2; exit 2; }
cd "$(git rev-parse --show-toplevel)"
if [ -z "$cmd" ]; then
    cargo fmt   # formatting rewrites files, so it runs here and the result is pushed
    cmd="cargo clippy --release --all-targets -- -D warnings && cargo test --release"
    [ "$full" = 0 ] || cmd="$cmd && cargo clippy --release --no-default-features --all-targets -- -D warnings && cargo test --release --no-default-features"
fi

# Install what a bare machine lacks (a no-op on the snapshot). `boat push`
# keeps each file's original timestamp, and the snapshot's target directory
# was built from older sources, so cargo could take an edited file for an
# unchanged one: stamp every pushed file as new first. In --save mode, drop
# the checkout afterwards so the snapshot keeps only packages, toolchain and
# target directory.
remote='set -euo pipefail
pkgs="libasound2-dev libudev-dev libfontconfig1-dev libfreetype6-dev pkg-config"
missing=$(for p in $pkgs; do dpkg-query -W -f="\${Status}" "$p" 2>/dev/null | grep -q "install ok installed" || echo "$p"; done)
if [ -n "$missing" ]; then
    # The image has a third-party package source whose release info changes.
    sudo apt-get update -qq --allow-releaseinfo-change || true
    sudo DEBIAN_FRONTEND=noninteractive NEEDRESTART_SUSPEND=1 apt-get install -y -qq $missing >/dev/null
fi
# The image keeps rustup in /usr/local, and its stable lags behind; CI
# builds with current stable.
export CARGO_HOME=/usr/local/cargo
rustup -q toolchain install stable --profile minimal -c clippy -c rustfmt >/dev/null
export CARGO_TARGET_DIR=$HOME/target CARGO_TERM_COLOR=always
find . -type f -exec touch -c {} +
echo "boat-check: $(rustc --version) on $(nproc) vCPUs" >&2
'"$cmd"
[ "$save" = 0 ] || remote="$remote"' && dir=$PWD && cd && rm -rf "$dir"'

from=()
if boat snapshots --json 2>/dev/null | jq -e --arg n "$snapshot" '.named[]? | select(.name == $n)' >/dev/null; then
    from=(--from "$snapshot")
else
    echo "boat-check: no $snapshot snapshot yet, so this is a cold build; --save makes one" >&2
fi
keep=()
[ "$save" = 0 ] || keep=(--keep)
log=$(mktemp "${TMPDIR:-/tmp}/boat-check.XXXXXX")
trap 'rm -f "$log"' EXIT
status=0
boat run --no-env --type "$type" ${from[@]+"${from[@]}"} ${keep[@]+"${keep[@]}"} --push . -- "$remote" 2> >(tee "$log" >&2) || status=$?
wait   # let tee finish writing the log
if [ "$save" = 1 ]; then
    id=$(grep -o -E 'bx_[a-z0-9]+' "$log" | head -n 1)
    if [ "$status" = 0 ] && [ -n "$id" ]; then
        # The save runs in the background; the snapshot keeps its own copy once it is done.
        boat snapshot "$id" "$snapshot" >/dev/null && echo "boat-check: saving $id as $snapshot" >&2
        for _ in $(seq 1 120); do
            state=$(boat snapshots --json | jq -r --arg n "$snapshot" '[.named[]? | select(.name == $n) | (.status // .state // "")][0] // ""')
            case "$state" in ready|saved|completed) break ;; esac
            sleep 5
        done
        echo "boat-check: $snapshot is ${state:-unknown}" >&2
        case "$state" in
            ready|saved|completed) boat delete "$id" --yes --json >/dev/null ;;
            *) boat stop "$id" --json >/dev/null; echo "boat-check: stopped $id instead of deleting it; once boat snapshots shows $snapshot ready: boat delete $id --yes" >&2 ;;
        esac
    elif [ -n "$id" ]; then
        boat delete "$id" --yes --json >/dev/null
    fi
fi
exit "$status"
}
main "$@"; exit
