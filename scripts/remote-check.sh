#!/usr/bin/env bash
# Run the hand-off checks on a Namespace cloud instance instead of this Mac.
#
#   scripts/remote-check.sh            cargo fmt here, then clippy + tests remotely
#   scripts/remote-check.sh --full     also the --no-default-features clippy + tests
#   scripts/remote-check.sh -- CMD...  run CMD remotely in the synced checkout
#                                      (one quoted argument runs as a shell line)
#
# The current working tree (tracked files plus untracked, non-ignored ones,
# uncommitted edits included) is copied to a fresh Linux instance. Its
# toolchain, cargo registry and target directory live on a Namespace cache
# volume shared by every run, so dependencies build once. Each run gets its
# own instance, so agents never wait on each other's cargo lock. The instance
# is destroyed when the run ends. Needs `nsc` installed and logged in; see
# docs/development.md#remote-checks.
#
# Environment overrides: VC_REMOTE_SHAPE (default 8x16), VC_REMOTE_DURATION
# (default 90m, the instance's maximum life), VC_REMOTE_CACHE_TAG (default
# voxelcraft-check), VC_REMOTE_KEEP=1 to leave the instance running.
set -euo pipefail

shape=${VC_REMOTE_SHAPE:-8x16}
duration=${VC_REMOTE_DURATION:-90m}
tag=${VC_REMOTE_CACHE_TAG:-voxelcraft-check}
full=0
fmt=1
cmd=""

while [ $# -gt 0 ]; do
    case "$1" in
        --full) full=1 ;;
        --no-fmt) fmt=0 ;;
        --)
            shift
            # One argument is a shell command line (`-- 'a && b'`); several
            # are one command whose arguments keep their boundaries.
            if [ $# -eq 1 ]; then cmd=$1; else cmd=$(printf '%q ' "$@"); fi
            break ;;
        -h|--help) sed -n '2,19p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
        *) echo "remote-check: unknown option $1 (try --help)" >&2; exit 2 ;;
    esac
    shift
done

command -v nsc >/dev/null || { echo "remote-check: nsc is not installed (docs/development.md#remote-checks)" >&2; exit 2; }
cd "$(git rev-parse --show-toplevel)"

if [ -z "$cmd" ]; then
    cmd="cargo clippy --release --all-targets -- -D warnings && cargo test --release"
    if [ "$full" = 1 ]; then
        cmd="$cmd && cargo clippy --release --no-default-features --all-targets -- -D warnings && cargo test --release --no-default-features"
    fi
    # Formatting rewrites files, so it runs here and the result is uploaded.
    if [ "$fmt" = 1 ]; then cargo fmt; fi
fi

work=$(mktemp -d "${TMPDIR:-/tmp}/remote-check.XXXXXX")
id=""
cleanup() {
    # An interrupt can land after `nsc create` wrote the ID but before we read it.
    if [ -z "$id" ] && [ -s "$work/cid" ]; then id=$(tr -cd 'a-z0-9' <"$work/cid"); fi
    if [ -n "$id" ] && [ "${VC_REMOTE_KEEP:-0}" != 1 ]; then
        nsc destroy "$id" --force >/dev/null 2>&1 || echo "remote-check: could not destroy instance $id; run: nsc destroy $id --force" >&2
    elif [ -n "$id" ]; then
        echo "remote-check: instance $id kept (nsc ssh $id; nsc destroy $id --force)" >&2
    fi
    rm -rf "$work"
}
trap cleanup EXIT
trap 'exit 130' INT TERM

echo "remote-check: creating a $shape instance (cache volume $tag)" >&2
# Namespace answers "not supported" while the workspace's concurrency is in
# use (other repositories' CI shares it), so wait and retry for a while.
attempt=0
until nsc create --bare --machine_type "$shape" --duration "$duration" \
        --volume "cache:$tag:/cache:20gb" --purpose "VoxelCraft remote check" \
        --label "voxelcraft=remote-check" --cidfile "$work/cid" >"$work/create.log" 2>&1; do
    attempt=$((attempt + 1))
    if ! grep -q "not supported" "$work/create.log" || [ "$attempt" -ge "${VC_REMOTE_RETRIES:-20}" ]; then
        cat "$work/create.log" >&2
        exit 1
    fi
    echo "remote-check: no capacity yet (attempt $attempt), retrying in 30 s" >&2
    sleep 30
done
id=$(cat "$work/cid")
echo "remote-check: instance $id" >&2

# Upload the working tree. Remote files get the upload time as their mtime,
# so cargo always rebuilds this crate and build.rs regenerates its paths.
git ls-files -z --cached --others --exclude-standard |
    while IFS= read -r -d '' f; do if [ -e "$f" ] || [ -L "$f" ]; then printf '%s\0' "$f"; fi; done |
    COPYFILE_DISABLE=1 tar --null -T - --no-xattrs -czf - |
    nsc ssh -T "$id" 'rm -rf /work && mkdir -p /work && tar -xzmf - -C /work'

# System packages go on every (fresh) instance as root. The checks run as an
# ordinary user, because tests of unreadable files can't fail for root. The
# toolchain installs once into the cache volume, owned by that user.
setup_script=$(cat <<'EOS'
set -euo pipefail
apk add -q build-base alsa-lib-dev eudev-dev pkgconf >/dev/null
id -u builder >/dev/null 2>&1 || adduser -D -u 1000 -s /bin/bash builder
mkdir -p /cache/rustup /cache/cargo /cache/target
[ "$(stat -c %u /cache/cargo)" = 1000 ] || chown -R 1000:1000 /cache
chown -R 1000:1000 /work
EOS
)
user_script=$(cat <<'EOS'
set -euo pipefail
export RUSTUP_HOME=/cache/rustup CARGO_HOME=/cache/cargo CARGO_TARGET_DIR=/cache/target
export PATH="$CARGO_HOME/bin:$PATH" CARGO_TERM_COLOR=always HOME=/home/builder
if ! command -v rustup >/dev/null; then
    curl -sSf https://sh.rustup.rs | sh -s -- -y -q --no-modify-path --profile minimal \
        --default-toolchain stable -c clippy -c rustfmt >/dev/null
fi
rustup -q update stable >/dev/null 2>&1 || true
cd /work
echo "remote-check: $(rustc --version) on $(nproc) vCPUs" >&2
EOS
)
status=0
{
    printf '%s\n' "$setup_script"
    # Sent as base64, so no line of the command can end the transfer early.
    printf 'printf %%s %s | base64 -d > /home/builder/check.sh\n' \
        "$(printf '%s\n%s\n' "$user_script" "$cmd" | base64 | tr -d '\n')"
    printf 'exec su builder -s /bin/bash -c "bash /home/builder/check.sh"\n'
} | nsc ssh -T "$id" 'bash -s' || status=$?
if [ "$status" = 0 ]; then
    echo "remote-check: passed" >&2
else
    echo "remote-check: FAILED (exit $status)" >&2
fi
exit "$status"
