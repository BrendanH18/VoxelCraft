#!/usr/bin/env bash
# Run the hand-off checks in this worktree's own long-lived Boat sandbox.
#
#   scripts/boat-check.sh            cargo fmt here, then clippy and tests in Boat
#   scripts/boat-check.sh --full     also the --no-default-features clippy and tests
#   scripts/boat-check.sh -- CMD...  run CMD in the checkout instead
#   scripts/boat-check.sh --save     rebuild the warm snapshot voxelcraft-warm from origin's
#                                    default branch, in a fresh sandbox that runs no worktree code
#   scripts/boat-check.sh --status   show this worktree's sandbox
#   scripts/boat-check.sh --stop     stop it (the next check resumes it)
#   scripts/boat-check.sh --done     delete it; run this when the agent's work ends
#
# The first check creates voxelcraft-check-<worktree> from voxelcraft-warm (system libraries,
# Rust, a checkout of the default branch and its built target directory), or a bare sandbox
# when there is no snapshot, and records its id in this worktree's git directory. Later checks
# reuse it: each pushes the working tree to a fresh directory, syncs it into the checkout at
# /home/user/voxelcraft (deleted files go; only changed files get a new timestamp, so cargo
# rebuilds exactly what changed) and runs there with `boat exec --timeout 0`, keeping cargo's
# target directory between runs. The sandbox stops as soon as each check ends (owner,
# 2026-11-22): a stopped sandbox bills nothing, and a resume within 10 minutes of the stop (or
# while it is still stopping) takes back the same machine without using a start. While a check
# runs, Boat's auto-stop (a deadline, not an idle timer) is kept 30 minutes ahead and renewed
# every 10 by a loop that ends with this script, so a killed run stops within 30 minutes. One
# sandbox per worktree; agents sharing a worktree each set VC_BOAT_SLOT. Overrides:
# VC_BOAT_TYPE (default large for every mode, owner, 2026-11-21: measured from the same warm
# state, xlarge took 450 s to large's 197 s for the test build and 1543 s to 1006 s for the
# full chain, at about 2.8 times the price), VC_BOAT_IDLE (seconds to keep the sandbox up after
# a check, through auto-stop, instead of stopping it; default 0), VC_BOAT_WAIT (seconds to wait
# for a machine once a minute of fail-fast tries found none; default 900, 0 gives up instead),
# VC_BOAT_SNAPSHOT.
# See docs/development.md#boat-checks.
main() {
set -euo pipefail
export BOAT_NO_UPDATE=1
snapshot=${VC_BOAT_SNAPSHOT:-voxelcraft-warm}
type=${VC_BOAT_TYPE:-large}
# Idle time bills like work, while a stopped sandbox bills nothing and is taken back without a
# start for 10 minutes, so every size stops the moment a check ends (owner, 2026-11-22; it
# replaces the 2026-11-21 idle timeouts of 15 minutes, 5 on xlarge). VC_BOAT_IDLE > 0 instead
# leaves the sandbox up that many seconds, through auto-stop.
idle=${VC_BOAT_IDLE:-0}
slot=${VC_BOAT_SLOT:-}
# When a minute of fail-fast tries finds no machine, one start waits for Boat to free one, for at
# most this many seconds (2026-11-23: busy moments outlasted the minute while machines freed
# within a few).
wait_for=${VC_BOAT_WAIT:-900}
# While a check runs, auto-stop is kept $budget seconds ahead and renewed every $renew seconds
# by a loop that ends with this script (owner, 2026-11-22; it was two hours, renewed every 30
# minutes, and outlived a killed script), so a killed run stops within $budget seconds.
budget=1800 renew=600
mode=checks action=run cmd=""
while [ $# -gt 0 ]; do
    case "$1" in
        --full) mode=full ;;
        --save|--status|--stop|--done)
            [ "$action" = run ] || { echo "boat-check: give one of --save, --status, --stop or --done" >&2; exit 2; }
            action=${1#--} ;;
        --) shift
            [ $# -gt 0 ] || { echo "boat-check: -- needs a command" >&2; exit 2; }
            if [ $# -eq 1 ]; then cmd=$1; else cmd=$(printf '%q ' "$@"); fi
            [ -n "$cmd" ] || { echo "boat-check: -- needs a command" >&2; exit 2; }
            mode=cmd
            break ;;
        -h|--help) sed -n '2,31p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
        *) echo "boat-check: unknown option $1 (try --help)" >&2; exit 2 ;;
    esac
    shift
done
[ "$action" = run ] || [ "$mode" = checks ] || { echo "boat-check: --$action takes no --full or command" >&2; exit 2; }
case "$slot" in *[!A-Za-z0-9_-]*) echo "boat-check: VC_BOAT_SLOT may use letters, digits, - and _" >&2; exit 2 ;; esac
case "$idle" in ''|*[!0-9]*) echo "boat-check: VC_BOAT_IDLE is a number of seconds" >&2; exit 2 ;; esac
case "$wait_for" in ''|*[!0-9]*) echo "boat-check: VC_BOAT_WAIT is a number of seconds" >&2; exit 2 ;; esac
for tool in boat jq git; do
    command -v "$tool" >/dev/null || { echo "boat-check: $tool is not installed" >&2; exit 2; }
done
cd "$(git rev-parse --show-toplevel)"
state_dir=$(git rev-parse --path-format=absolute --git-path boat-check)
mkdir -p "$state_dir"
id_file=$state_dir/sandbox${slot:+-$slot}
# Creates that were cut off and not yet found, one "name<TAB>epoch seconds<TAB>check|build" line
# each, for later runs to settle.
pending_file=$state_dir/pending${slot:+-$slot}
lock=$state_dir/lock${slot:+-$slot}
# Every sandbox says which project it belongs to (owner, 2026-11-21). Each creation adds the
# time and this run's pid, so an interrupted create finds only the sandbox it made, never one
# another clone with the same directory name and slot made.
name="voxelcraft-check-$(basename "$PWD" | tr -c 'A-Za-z0-9_\n-' '-' | cut -c1-80)${slot:+-$slot}"

# run_name BASE: BASE plus this run's time and pid, BASE cut so the whole fits Boat's
# 120-character limit on names; the suffix stays whole, since cleanup finds the sandbox by it.
run_name() {
    local suffix
    suffix="-$(date +%Y%m%d-%H%M%S)-$$"
    echo "${1:0:$((120 - ${#suffix}))}$suffix"
}
id="" new_id=""
out_file=$state_dir/out${slot:+-$slot}-$$
extended=0 keepalive="" job="" watchdog="" creating="" build="" saving=0 locked=0 tree=""
end_job() {   # end_job PID: a background job and its children
    [ -n "$1" ] || return 0
    pkill -P "$1" 2>/dev/null || true
    kill "$1" 2>/dev/null || true
}
stop_keepalive() { end_job "$keepalive"; keepalive=""; }   # the keepalive loop and its sleep
# stop_now, park and cleanup run from the exit trap.
# shellcheck disable=SC2329
# stop_now ID: stop a sandbox. If Boat refuses, set its auto-stop a minute away instead and
# return 1; if Boat refuses that too, return 2: its last auto-stop (up to $budget s) stands.
stop_now() {
    boat stop "$1" --json >/dev/null 2>&1 && return 0
    boat extend "$1" --ttl 60 --json >/dev/null 2>&1 && return 1
    return 2
}
# shellcheck disable=SC2329
# left_running ID RC FIX: say what a failed stop_now (RC) left, and the command FIX to run.
left_running() {
    if [ "$2" = 1 ]; then
        echo "boat-check: couldn't stop $1, so its auto-stop is now a minute away: $3" >&2
    else
        echo "boat-check: couldn't stop $1 or move its auto-stop up, so it runs until its last auto-stop (up to $budget s): $3" >&2
    fi
}
# shellcheck disable=SC2329
# park ID: stop the check sandbox, or with VC_BOAT_IDLE set, set its auto-stop that far ahead.
park() {
    if [ "$idle" -gt 0 ]; then
        boat extend "$1" --ttl "$idle" --json >/dev/null 2>&1 \
            || echo "boat-check: couldn't set auto-stop on $1: boat extend $1 --ttl $idle" >&2
    else
        local rc=0
        stop_now "$1" || rc=$?
        [ "$rc" = 0 ] || left_running "$1" "$rc" "boat stop $1"
    fi
}
# shellcheck disable=SC2329
cleanup() {
    set +e
    trap '' INT TERM HUP   # a second Ctrl-C must not cut the cleanup short
    stop_keepalive
    end_job "$watchdog"
    in_flight=0; [ -z "$job" ] || in_flight=1
    end_job "$job"
    rm -f "$out_file" "$out_file.timeout"
    # Interrupted while Boat made a sandbox: it may exist with no id here yet, so find it by
    # its name, in any state but stopped (Boat's state groups don't name every starting state),
    # taking the newest, so an older stopped sandbox of the name is never picked. A create
    # request still in flight can land after the first look, so then look for 20 seconds.
    if [ -n "$creating" ]; then
        tries=1; [ "$in_flight" = 0 ] || tries=10
        made=""
        while [ "$tries" -gt 0 ]; do
            made=$(boat list --all --json 2>/dev/null | jq -r --arg n "$creating" \
                '[.sandboxes[]? | select(.name == $n and .state != "stopped")] | sort_by(.createdAt // "") | last | .id // empty' 2>/dev/null)
            [ -z "$made" ] || break
            tries=$((tries - 1))
            [ "$tries" = 0 ] || sleep 2
        done
        if [ -z "$made" ]; then
            if [ "$in_flight" = 1 ]; then
                kind=check; [ "$action" != save ] || kind=build
                printf '%s\t%s\t%s\n' "$creating" "$(date +%s)" "$kind" >> "$pending_file"
                echo "boat-check: found no sandbox named $creating yet; the next run adopts or deletes it if it lands (it stops by itself within $budget s)" >&2
            fi
        elif [ "$action" = save ]; then build=$made
        else id=$made extended=1; echo "$id" > "$id_file"
        fi
    fi
    # Stop the sandbox (or with VC_BOAT_IDLE, leave it that long), whatever happened to the check.
    if [ "$extended" = 1 ] && [ -n "$id" ]; then park "$id"; fi
    # The --save build sandbox is deleted, or only stopped while a save from it may still be
    # running (an unfinished save needs its source; a stopped sandbox costs nothing).
    if [ -n "$build" ]; then
        if [ "$saving" = 1 ]; then
            rc=0
            stop_now "$build" || rc=$?
            if [ "$rc" = 0 ]; then
                echo "boat-check: stopped the build sandbox $build; once boat snapshots shows $snapshot ready from it: boat delete $build --yes" >&2
            else
                left_running "$build" "$rc" "boat stop $build, then once boat snapshots shows $snapshot ready from it: boat delete $build --yes"
            fi
        elif ! boat delete "$build" --yes --json >/dev/null 2>&1; then
            rc=1
            boat extend "$build" --ttl 60 --json >/dev/null 2>&1 || rc=2
            left_running "$build" "$rc" "boat delete $build --yes"
        fi
    fi
    [ -z "$tree" ] || rm -rf "$tree"
    [ "$locked" = 0 ] || rm -rf "$lock"   # only a lock this run took
}
trap cleanup EXIT
trap 'exit 130' INT TERM HUP
# --status only reads, so it needs no lock.
if [ "$action" = status ]; then
    id=$(cat "$id_file" 2>/dev/null || true)
    if [ -z "$id" ]; then echo "boat-check: no sandbox for this worktree${slot:+ (slot $slot)}"; exit 0; fi
    echo "boat-check: $id"
    boat info "$id"
    exit 0
fi
# One run at a time per slot, so two checks never race to create the slot's sandbox. Only
# `mkdir` takes the lock, and the exit trap (set above, before the lock is taken) releases it
# only when this run took it. A lock a killed run left behind is reported with its path, never
# taken over, so no run can remove a lock another run holds.
if ! mkdir "$lock" 2>/dev/null; then
    holder=$(cat "$lock/pid" 2>/dev/null || true)
    if [ -n "$holder" ] && ! kill -0 "$holder" 2>/dev/null; then
        echo "boat-check: a boat-check that has ended (pid $holder) left its lock; remove $lock and run again" >&2
    else
        echo "boat-check: another boat-check (pid ${holder:-starting}) holds this worktree's sandbox lock; wait for it, or give this agent its own VC_BOAT_SLOT. If no boat-check is running, remove $lock" >&2
    fi
    exit 2
fi
locked=1
echo $$ > "$lock/pid"
# Read under the lock, so a run that waited never misses the sandbox another run just made.
id=$(cat "$id_file" 2>/dev/null || true)
# Creates cut off by earlier runs may have landed after those runs gave up looking: adopt the
# first check sandbox as this slot's if it has none, and delete the rest, --save build
# sandboxes included. A name not found is kept for a day, judged only from a listing Boat
# answered in full; without one, every name is kept for the next run.
if [ -s "$pending_file" ]; then
    listing=$(boat list --all --json 2>/dev/null) \
        && printf '%s\n' "$listing" | jq -e '.sandboxes | type == "array"' >/dev/null 2>&1 \
        || listing=""
    if [ -n "$listing" ]; then
        keep="" now=$(date +%s)
        while IFS=$'\t' read -r pending since kind; do
            [ -n "$pending" ] || continue
            case "$since" in ''|*[!0-9]*) since=$now ;; esac
            line="$pending"$'\t'"$since"$'\t'"${kind:-check}"$'\n'
            late=$(printf '%s\n' "$listing" | jq -r --arg n "$pending" \
                '[.sandboxes[] | select(.name == $n)] | sort_by(.createdAt // "") | last | .id // empty')
            if [ -z "$late" ]; then
                [ $((now - since)) -gt 86400 ] || keep="$keep$line"
            elif [ -z "$id" ] && [ "${kind:-check}" = check ]; then
                echo "boat-check: adopting $late ($pending), which a cut-off start made late" >&2
                # Marked like any sandbox this run started, so the exit stops it even when this
                # run (a --save, say) never uses it.
                id=$late extended=1; echo "$id" > "$id_file"
            elif [ "$late" != "$id" ]; then
                echo "boat-check: deleting $late ($pending), which a cut-off start made late" >&2
                boat delete "$late" --yes --json </dev/null >/dev/null 2>&1 || keep="$keep$line"
            fi
        done < "$pending_file"
        if [ -n "$keep" ]; then printf '%s' "$keep" > "$pending_file.new" && mv "$pending_file.new" "$pending_file"
        else rm -f "$pending_file"; fi
    fi
fi

# The sandbox's state and size ("stopped large"), or "gone" when Boat no longer knows it. Any
# other failure (network, auth, rate limit) is an error, so a passing hiccup never orphans a
# sandbox or spends a start.
sandbox_info() {
    local out
    if out=$(boat info "$1" --json 2>&1); then
        printf '%s\n' "$out" | jq -r '"\(.sandbox.state // "unknown") \(.sandbox.type // "")"'
        return
    fi
    case "$out" in *'"not_found"'*) echo gone; return 0 ;; esac
    echo "boat-check: couldn't read sandbox $1 from Boat: $out" >&2
    return 1
}
sandbox_state() { local info; info=$(sandbox_info "$1") || return 1; echo "${info%% *}"; }

# Each create or resume uses one of the account's daily starts; reusing a running sandbox uses none.
start_failed() {
    case "$2" in *no_ready_machine*)
        echo "boat-check: $1: Boat had no machine ready after a minute of tries and ${wait_for} s of waiting; try again later (VC_BOAT_WAIT sets the wait)." >&2
        exit 3 ;;
    esac
    local why
    why=$(printf '%s\n' "$2" | jq -r 'select(.event == "error") | .error // empty' 2>/dev/null || true)
    echo "boat-check: $1 failed:" >&2
    printf '%s\n' "${why:-$2}" >&2
    echo "boat-check: boat limits shows the starts left today; tell the owner rather than retrying" >&2
    exit 3
}
# With --fail-fast, a start that finds no ready machine creates nothing and isn't counted against
# the start limits, so retry it a few times. A failed attempt can leave Boat finishing a resume for
# a moment ("resume already in progress"), which a retry also waits out (as jev_fsd's script does),
# and a resume Boat can't yet match to a stop in progress fails with sandbox_stopping until it can.
# Each call runs in the background and is waited on, so a signal reaches the cleanup at once
# rather than after the call (a create waits until the sandbox is ready).
fast_start() {   # fast_start WHAT boat ARGS...; leaves the output in $out
    local what=$1 attempt rc
    shift
    for attempt in 1 2 3 4 5 6; do
        "$@" --fail-fast --json >"$out_file" 2>&1 </dev/null &
        job=$!
        rc=0; wait "$job" || rc=$?
        job=""
        out=$(cat "$out_file" 2>/dev/null || true)
        if [ "$rc" = 0 ]; then return 0; fi
        case "$out" in
            *no_ready_machine*) [ "$attempt" = 6 ] || echo "boat-check: no machine ready for $what; trying again in 10 s" >&2 ;;
            *"already in progress"*|*sandbox_stopping*) [ "$attempt" = 6 ] || echo "boat-check: Boat is still busy with $what; trying again in 10 s" >&2 ;;
            *) return 1 ;;
        esac
        [ "$attempt" = 6 ] || sleep 10
    done
    case "$out" in *no_ready_machine*|*"already in progress"*|*sandbox_stopping*) ;; *) return 1 ;; esac
    [ "$wait_for" -gt 0 ] || return 1
    # Still none: starts without --fail-fast, which Boat holds until a machine frees up, until
    # $wait_for seconds have passed. A busy answer (a resume an earlier try left in progress, a
    # stop not yet matched) is tried again 10 s later. A create cut off may still land, so job
    # stays set then and the cleanup looks for the sandbox by name; a resume cut off is stopped.
    echo "boat-check: still no machine ready for $what; waiting up to ${wait_for} s for one" >&2
    local deadline left
    deadline=$(($(date +%s) + wait_for))
    while :; do
        left=$((deadline - $(date +%s)))
        if [ "$left" -le 0 ]; then
            out="no_ready_machine: no machine within ${wait_for} s"
            return 1
        fi
        rm -f "$out_file.timeout"
        "$@" --json >"$out_file" 2>&1 </dev/null &
        job=$!
        # The marker is written only when the sleep ran out: ending the watchdog kills the
        # sleep first, and the && then skips the rest.
        (sleep "$left" && : >"$out_file.timeout" && end_job "$job") >/dev/null 2>&1 </dev/null &
        watchdog=$!
        rc=0; wait "$job" || rc=$?
        end_job "$watchdog"; watchdog=""
        out=$(cat "$out_file" 2>/dev/null || true)
        # A start that succeeded counts even if the deadline passed as it finished.
        if [ "$rc" = 0 ]; then rm -f "$out_file.timeout"; job=""; return 0; fi
        if [ -e "$out_file.timeout" ]; then
            rm -f "$out_file.timeout"
            out="no_ready_machine: no machine within ${wait_for} s"
            return 1
        fi
        case "$out" in
            *no_ready_machine*|*"already in progress"*|*sandbox_stopping*)
                job=""
                left=$((deadline - $(date +%s)))
                [ "$left" -le 10 ] || left=10
                [ "$left" -gt 0 ] || continue
                echo "boat-check: Boat is still busy with $what; trying again in $left s" >&2
                sleep "$left" ;;
            *) return 1 ;;
        esac
    done
}
# new_sandbox NAME [boat new ARGS...]: sets new_id to the new sandbox's id. Set creating=NAME
# around it, so a run interrupted before the id comes back still finds the sandbox and cleans
# it up. It runs in this shell, not $(...), so a signal interrupts it at once.
new_sandbox() {
    local sandbox_name=$1
    shift
    # --no-env: none of the account's stored secrets or logins go in.
    fast_start "a new sandbox" boat new --no-env --type "$type" --ttl "$budget" --name "$sandbox_name" "$@" \
        || start_failed "creating a sandbox" "$out"
    new_id=$(printf '%s\n' "$out" | jq -r 'select(.event == "ready") | .id // empty' 2>/dev/null | head -n 1 || true)
    [ -n "$new_id" ] || start_failed "creating a sandbox" "$out"
}

# Runs in the sandbox as `bash -c`: $1 mode (checks, full, cmd or build), $2 push directory,
# $3 command.
# shellcheck disable=SC2016
remote='set -euo pipefail
mode=$1 push=$2 cmd=$3
repo=/home/user/voxelcraft
say() { echo "boat-check: $*" >&2; }
# A check whose CLI was killed may still be running here; wait for it rather than pull its tree away.
exec 9>"$HOME/.boat-check.lock"
flock -w 900 9 || { say "an earlier check is still running in this sandbox; try again later"; exit 2; }
# Pushes older than 12 hours (from checks that stopped before syncing) go first.
find /home/user/push -mindepth 1 -maxdepth 1 -mmin +720 -exec rm -rf {} + 2>/dev/null || true

# System libraries and Rust: already in the warm snapshot, installed here on a bare sandbox.
pkgs="libasound2-dev libudev-dev libfontconfig1-dev libfreetype6-dev pkg-config rsync"
missing=$(for p in $pkgs; do dpkg-query -W -f="\${Status}" "$p" 2>/dev/null | grep -q "install ok installed" || echo "$p"; done)
if [ -n "$missing" ]; then
    # The image has a third-party package source whose release info changes.
    sudo apt-get update -qq --allow-releaseinfo-change || true
    sudo DEBIAN_FRONTEND=noninteractive NEEDRESTART_SUSPEND=1 apt-get install -y -qq $missing >/dev/null
fi
# The image keeps rustup in /usr/local.
export CARGO_HOME=/usr/local/cargo
export CARGO_TARGET_DIR=$HOME/target CARGO_TERM_COLOR=always

# The checkout stays at one path, so cargo keeps its build of it. Sync the pushed tree into it:
# deleted files go, and only files whose contents changed are rewritten, with the current time,
# so cargo rebuilds what changed and nothing else (boat push keeps the computer'\''s timestamps).
mkdir -p "$repo"
rsync -rlpc --delete "$push/" "$repo/"
rm -rf "$push"
cd "$repo"
# The toolchain rust-toolchain.toml pins, as CI and the release builds use.
channel=stable
[ ! -f rust-toolchain.toml ] || channel=$(sed -n '\''s/^channel *= *"\(.*\)"/\1/p'\'' rust-toolchain.toml)
rustup -q toolchain install "$channel" --profile minimal -c clippy -c rustfmt >/dev/null
say "$(rustc --version) on $(nproc) vCPUs"

checks="cargo clippy --profile ci --all-targets --locked -- -D warnings && cargo test --profile ci --locked"
full="cargo clippy --profile ci --no-default-features --all-targets --locked -- -D warnings && cargo test --profile ci --no-default-features --locked"
case "$mode" in
    checks) ( eval "$checks" ) ;;
    full|build) ( eval "$checks" ) && ( eval "$full" ) ;;
    cmd) ( eval "$cmd" ) ;;
esac'

# `boat exec` prints JSON frames whenever its output isn't a terminal, so ask for them always and
# turn them back into the command's stdout, stderr and exit code. Each frame's text travels as
# one base64 line, so partial lines and control characters survive.
frames() {
    jq -rR --unbuffered '(fromjson? // {type: "text", data: (. + "\n")})
        | if .type == "stdout" then "O" + (.data | @base64)
          elif .type == "stderr" or .type == "text" then "E" + (.data | @base64)
          elif .type == "exit" then "X\(.exitCode // 1)"
          elif .event == "error" then "E" + ("boat: \(.error)\n" | @base64)
          else empty end'
}
unframe() {
    local line rc=""
    while IFS= read -r line; do
        case "$line" in
            O*) printf '%s' "${line#O}" | base64 -d ;;
            E*) printf '%s' "${line#E}" | base64 -d >&2 ;;
            X*) rc=${line#X} ;;
        esac
    done
    [ -n "$rc" ] || return 255
    return "$rc"
}
# run_remote ID MODE PUSH CMD: run the remote script in sandbox ID and return its exit code.
# Auto-stop is pushed back to $budget seconds every $renew seconds meanwhile, so a long check
# never reaches it, by a loop that checks this script is still alive first: if the script is
# killed outright, the loop ends and the sandbox stops within $budget seconds.
run_remote() {
    local rc=0
    # Its own stdio, so its sleep never holds the caller's output pipe open after the script ends.
    ( while sleep "$renew"; do
          kill -0 "$$" 2>/dev/null || exit 0
          boat extend "$1" --ttl "$budget" --json >/dev/null 2>&1
      done ) </dev/null >/dev/null 2>&1 &
    keepalive=$!
    # In the background, because bash runs a signal trap only once a foreground command ends:
    # waiting on it instead lets TERM or HUP reach the cleanup at once, not after the check.
    (
        set +e
        boat exec "$1" --timeout 0 --json -- bash -c "$remote" boat-check "$2" "$3" "$4" | frames | unframe
        codes=("${PIPESTATUS[@]}")
        rc=${codes[2]}
        if [ "$rc" = 255 ]; then   # no exit frame: the CLI failed before or while running the command
            rc=${codes[0]}
            [ "$rc" != 0 ] || rc=1
            echo "boat-check: boat exec ended without the command's exit code (exit ${codes[0]})" >&2
        fi
        exit "$rc"
    ) &
    job=$!
    wait "$job" || rc=$?
    job=""
    stop_keepalive
    return "$rc"
}
push_dir() { echo "/home/user/push/$(date +%Y%m%d-%H%M%S)-$$"; }

case "$action" in
    save)
        # A fresh bare sandbox builds origin's default branch, so no worktree's changes reach
        # the snapshot every later sandbox starts from. Uses one start.
        # The server's current default branch, fetched by name so no stale origin/HEAD is used.
        git fetch -q origin HEAD || { echo "boat-check: couldn't fetch origin's default branch" >&2; exit 1; }
        base=$(git rev-parse --verify FETCH_HEAD)
        tree=$(mktemp -d "${TMPDIR:-/tmp}/boat-check-save.XXXXXX")
        git archive "$base" | tar -x -C "$tree"
        echo "boat-check: building $snapshot from origin's default branch ($(git rev-parse --short "$base")) in a fresh sandbox" >&2
        # A name only this run uses, so an interrupted create is found and deleted.
        creating=$(run_name "voxelcraft-${snapshot#voxelcraft-}-build")
        new_sandbox "$creating"; build=$new_id
        creating=""
        push=$(push_dir)
        boat push "$build" "$tree" --to "$push" >/dev/null
        run_remote "$build" build "$push" "" || { echo "boat-check: the build failed, so $snapshot was not saved" >&2; exit 1; }
        # The save runs in the background. Wait for this sandbox's save of the name
        # (sourceSandboxId) to read ready; an older save of the name doesn't count. Auto-stop
        # is put back $budget seconds ahead to cover the 10-minute wait.
        echo "boat-check: saving $build as $snapshot" >&2
        boat extend "$build" --ttl "$budget" --json >/dev/null 2>&1 || true
        saving=1
        boat snapshot "$build" "$snapshot" >/dev/null || { echo "boat-check: saving $snapshot failed" >&2; exit 1; }
        deadline=$((SECONDS + 600)) status=""
        while :; do   # poll once more after the deadline before giving up
            status=$(boat snapshots --json 2>/dev/null | jq -r --arg n "$snapshot" --arg id "$build" \
                '[.named[]? | select(.name == $n and .sourceSandboxId == $id) | .status][0] // ""' || true)
            case "$status" in ready|failed) break ;; esac
            [ "$SECONDS" -lt "$deadline" ] || break
            sleep 5
        done
        case "$status" in
            ready) echo "boat-check: saved $snapshot" >&2; saving=0; exit 0 ;;   # the exit trap deletes the build sandbox
            failed) echo "boat-check: saving $snapshot failed (boat snapshots shows why)" >&2; saving=0; exit 1 ;;
            *) echo "boat-check: $snapshot isn't ready after about 10 minutes" >&2; exit 1 ;;
        esac ;;
    stop)
        [ -n "$id" ] || { echo "boat-check: no sandbox for this worktree${slot:+ (slot $slot)}" >&2; exit 0; }
        boat stop "$id" --json >/dev/null || { echo "boat-check: couldn't stop $id: boat stop $id" >&2; exit 1; }
        echo "boat-check: stopped $id"
        exit 0 ;;
    done)
        [ -n "$id" ] || { echo "boat-check: no sandbox for this worktree${slot:+ (slot $slot)}" >&2; exit 0; }
        if boat delete "$id" --yes --json >/dev/null 2>&1 || [ "$(sandbox_state "$id")" = gone ]; then
            rm -f "$id_file"; id=""
            echo "boat-check: deleted this worktree's sandbox"
            exit 0
        fi
        echo "boat-check: couldn't delete $id: boat delete $id --yes" >&2
        exit 1 ;;
esac

# Formatting rewrites files, so it runs here and the result is pushed.
[ "$mode" = cmd ] || cargo fmt

# Find this slot's sandbox and get it running, or make one.
state="" size=""
read_state() {
    local info
    info=$(sandbox_info "$id") || return 1
    state=${info%% *} size=${info#"$state"}; size=${size# }
}
if [ -n "$id" ]; then
    read_state || { echo "boat-check: try again once Boat answers" >&2; exit 1; }
fi
# A sandbox not yet ready (starting, provisioning, cloning) is waited for, not replaced. A
# stopping one is resumed at once, which takes back its machine with no start, unless it must
# change size: that needs a new machine, so it waits for stopped first.
waiting() {
    case "$state" in
        init|provisioning|provisioned|cloning|starting|resuming) return 0 ;;
        stopping) [ "$size" != "$type" ] ;;
        *) return 1 ;;
    esac
}
if waiting; then
    for _ in $(seq 1 24); do
        sleep 5; read_state || exit 1
        waiting || break
    done
fi
if [ "$state" = stopping ] && waiting; then
    echo "boat-check: $id is still stopping, and resuming it as $type needs it stopped; try again in a minute" >&2
    exit 2
fi
case "$state" in
    ready|idle|running) ;;
    init|provisioning|provisioned|cloning|starting|resuming) echo "boat-check: $id is still $state; try again in a minute" >&2; exit 2 ;;
    stopped|stopping)
        if [ "$size" = "$type" ]; then
            # A plain resume: within 10 minutes of the stop (or during it) Boat usually takes
            # back the same machine, which uses no start.
            resize=()
            echo "boat-check: resuming $id ($state)" >&2
        else
            resize=(--type "$type")
            echo "boat-check: resuming $id as $type (it was ${size:-of unknown size}; a new size uses a start)" >&2
        fi
        extended=1   # before the resume, so an interruption once it has started still stops it
        if ! fast_start "resuming $id" boat resume "$id" --no-env ${resize[@]+"${resize[@]}"} --ttl "$budget"; then
            case "$out" in
                *snapshot_data_expired*) echo "boat-check: $id can't be resumed (its data expired); making a new one" >&2; state="" extended=0 ;;
                *) start_failed "resuming $id" "$out" ;;
            esac
        fi ;;
    "") ;;
    gone) echo "boat-check: Boat doesn't know $id any more; making a new sandbox" >&2; state="" ;;
    unknown) echo "boat-check: boat info $id gave no state; check it with boat info $id" >&2; exit 1 ;;
    *)
        echo "boat-check: $id is $state; deleting it and making a new one" >&2
        boat delete "$id" --yes --json >/dev/null 2>&1 || [ "$(sandbox_state "$id")" = gone ] \
            || { echo "boat-check: couldn't delete $id ($state): boat delete $id --yes, then run again" >&2; exit 1; }
        state="" ;;
esac
if [ -z "$state" ]; then
    rm -f "$id_file"
    from=()
    if boat snapshots --json 2>/dev/null | jq -e --arg n "$snapshot" '.named[]? | select(.name == $n and .status == "ready")' >/dev/null; then
        from=(--from "$snapshot")
    else
        echo "boat-check: no ready $snapshot snapshot, so this sandbox starts bare and builds everything; --save makes one" >&2
    fi
    creating=$(run_name "$name")
    echo "boat-check: creating $creating" >&2
    new_sandbox "$creating" ${from[@]+"${from[@]}"}; id=$new_id
    echo "$id" > "$id_file"
    creating=""
fi
extended=1   # from here on, the exit trap stops the sandbox
boat extend "$id" --ttl "$budget" --json >/dev/null
echo "boat-check: sandbox $id" >&2

# Push into a fresh directory; the remote script syncs it into the checkout.
push=$(push_dir)
boat push "$id" . --to "$push" >/dev/null
boat extend "$id" --ttl "$budget" --json >/dev/null   # a slow push mustn't eat into the first renewal
status=0
run_remote "$id" "$mode" "$push" "$cmd" || status=$?
exit "$status"
}
# The exit stops bash reading on, should this file change while the check runs.
# shellcheck disable=SC2317
{ main "$@"; exit; }
