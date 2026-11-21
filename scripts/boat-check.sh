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
# target directory) when it is ready, else from a bare machine. Overrides:
# VC_BOAT_TYPE (default large: the release test build needs 16 GB) and
# VC_BOAT_SNAPSHOT. See docs/development.md#boat-checks.
main() {
set -euo pipefail
export BOAT_NO_UPDATE=1
snapshot=${VC_BOAT_SNAPSHOT:-voxelcraft-warm}
cmd="" full=0 save=0
while [ $# -gt 0 ]; do
    case "$1" in
        --full) full=1 ;;
        --save) save=1 ;;
        --) shift
            [ $# -gt 0 ] || { echo "boat-check: -- needs a command" >&2; exit 2; }
            if [ $# -eq 1 ]; then cmd=$1; else cmd=$(printf '%q ' "$@"); fi
            break ;;
        -h|--help) sed -n '2,15p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
        *) echo "boat-check: unknown option $1 (try --help)" >&2; exit 2 ;;
    esac
    shift
done
for tool in boat jq; do
    command -v "$tool" >/dev/null || { echo "boat-check: $tool is not installed" >&2; exit 2; }
done
type=${VC_BOAT_TYPE:-large}
if [ -z "${VC_BOAT_TYPE:-}" ] && [ "$(boat limits --json 2>/dev/null | jq -r '[.machineTypes[]? | select(.type == "large") | .available | tostring][0] // "true"' || true)" = false ]; then
    type=default   # the free trial can't start large
    echo "boat-check: this account can't start large sandboxes yet; using default (8 GB), where the full release test build runs out of memory" >&2
fi
cd "$(git rev-parse --show-toplevel)"
if [ -z "$cmd" ]; then
    cargo fmt   # formatting rewrites files, so it runs here and the result is pushed
    # --locked, as CI runs them, so a stale Cargo.lock fails here too
    cmd="cargo clippy --release --all-targets --locked -- -D warnings && cargo test --release --locked"
    [ "$full" = 0 ] || cmd="$cmd && cargo clippy --release --no-default-features --all-targets --locked -- -D warnings && cargo test --release --no-default-features --locked"
fi

# Install what a bare machine lacks (a no-op on the snapshot). `boat push`
# keeps each file's original timestamp, and the snapshot's target directory
# was built from older sources, so cargo could take an edited file for an
# unchanged one: stamp every pushed file as new first. The command runs in a
# subshell on its own lines, so a `cd`, `;` or trailing comment in it can't
# change what follows. In --save mode, the checkout is then removed so the
# snapshot keeps only packages, toolchain and target directory.
remote='set -euo pipefail
workdir=$PWD
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
(
'"$cmd"'
)'
if [ "$save" = 1 ]; then
    remote="$remote"'
cd
if [ "$workdir" != "$HOME" ]; then rm -rf "$workdir"; fi'
fi

from=()
if boat snapshots --json 2>/dev/null | jq -e --arg n "$snapshot" '.named[]? | select(.name == $n and .status == "ready")' >/dev/null; then
    from=(--from "$snapshot")
else
    echo "boat-check: no ready $snapshot snapshot, so this is a cold build; --save makes one" >&2
fi
keep=()
[ "$save" = 0 ] || keep=(--keep)

log=$(mktemp "${TMPDIR:-/tmp}/boat-check.XXXXXX")
id="" handled=0 saving=0
sandbox_id() { awk 'match($0, /bx_[a-z0-9]+/) { print substr($0, RSTART, RLENGTH); exit }' "$log" 2>/dev/null || true; }
# A --save run keeps its sandbox. If the script stops before handling it,
# delete it; once a save has started, stop it instead, because the snapshot
# needs its source until the save reads ready.
cleanup() {
    set +e
    if [ "$save" = 1 ] && [ "$handled" = 0 ]; then
        [ -n "$id" ] || id=$(sandbox_id)
        if [ -n "$id" ] && [ "$saving" = 1 ]; then
            boat stop "$id" --json >/dev/null 2>&1
            echo "boat-check: stopped $id mid-save; once boat snapshots shows $snapshot ready: boat delete $id --yes" >&2
        elif [ -n "$id" ]; then
            boat delete "$id" --yes --json >/dev/null 2>&1 || echo "boat-check: delete sandbox $id: boat delete $id --yes" >&2
        fi
    fi
    rm -f "$log"
}
trap cleanup EXIT
trap 'exit 130' INT TERM

# A plain pipeline, so tee has finished writing the log when it returns.
set +e
boat run --no-env --type "$type" ${from[@]+"${from[@]}"} ${keep[@]+"${keep[@]}"} --push . -- "$remote" 2>&1 | tee "$log"
status=${PIPESTATUS[0]}
set -e
[ "$save" = 1 ] || exit "$status"

id=$(sandbox_id)
[ -n "$id" ] || { echo "boat-check: couldn't find the sandbox id in boat run's output" >&2; exit "$(( status ? status : 1 ))"; }
[ "$status" = 0 ] || exit "$status"   # the exit trap deletes the kept sandbox

# The save runs in the background. Wait for this sandbox's save of the name
# (sourceSandboxId) to read ready; an older save of the name doesn't count.
boat snapshot "$id" "$snapshot" >/dev/null || { echo "boat-check: saving $snapshot failed" >&2; exit 1; }
saving=1
echo "boat-check: saving $id as $snapshot" >&2
state=""
for _ in $(seq 1 120); do
    state=$(boat snapshots --json 2>/dev/null | jq -r --arg n "$snapshot" --arg id "$id" \
        '[.named[]? | select(.name == $n and .sourceSandboxId == $id) | .status][0] // ""' || true)
    case "$state" in ready|failed) break ;; esac
    sleep 5
done
case "$state" in
    ready)
        echo "boat-check: saved $snapshot" >&2
        handled=1
        boat delete "$id" --yes --json >/dev/null || echo "boat-check: delete sandbox $id: boat delete $id --yes" >&2 ;;
    failed)
        echo "boat-check: saving $snapshot failed (boat snapshots shows why)" >&2
        saving=0   # nothing left to protect: the exit trap deletes the sandbox
        exit 1 ;;
    *)
        echo "boat-check: $snapshot isn't ready after 10 minutes" >&2
        exit 1 ;;   # the exit trap stops the sandbox
esac
exit 0
}
main "$@"; exit
