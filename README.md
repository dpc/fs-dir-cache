# FS Dir Cache

A CLI tool for CIs and build scripts, making file system based
caching easy and correct (locking, eviction, etc.)

When working on build systems / CIs it's often a requirement to
utilize some best effort caching, with locking and eviction.

Not exactly rocket science, but non-trivial enough to not want
to implement and maintain ad-hoc.

`fs-dir-cache` aims to be a simple to use utility from inside
other scripts and programs taking care of the details.

## Example

This is an example, where a CI runner can persist files between runs,
and it's used to to reuse build artifacts between builds that are likely
building the same build to speed everything up:


```bash
#!/usr/bin/env bash

set -euo pipefail

job_name="$1"
shift 1

if [ -z "$job_name" ]; then
    >&2 "error: no job name"
    exit 1
fi

export FS_DIR_CACHE_LOCK_TIMEOUT_SECS="$((60 * 30))" # unlock after timeout in case our job fails misereably and/or hangs

export FS_DIR_CACHE_ROOT="$HOME/.cache/fs-dir-cache" # directory to hold all cache (sub)directories
export FS_DIR_CACHE_LOCK_ID="pid-$$-rnd-$RANDOM"     # acquire lock based on the current pid and something random (just in case pid gets reused)
export FS_DIR_CACHE_KEY_NAME="$job_name"             # the base name of our key

log_file="$FS_DIR_CACHE_ROOT/log"

fs-dir-cache gc unused --seconds "$((5 * 24 * 60 * 60))" # delete caches not used in more than a 5 days

export log_file # log when each job starte and ended
export job_name
src_dir=$(pwd)
export src_dir

# This bash command will be executed with a CWD set to the allocated directory
function run_in_cache() {
    echo "$(date --rfc-3339=seconds) RUN job=$job_name dir=$(pwd)" >> "$log_file"
    >&2 echo "$(date --rfc-3339=seconds) RUN job=$job_name dir=$(pwd)"
    CARGO_BUILD_TARGET_DIR="$(pwd)"
    export CARGO_BUILD_TARGET_DIR
    cd "$src_dir"

    function on_exit() {
        local exit_code=$?

        echo "$(date --rfc-3339=seconds) END job=$job_name code=$exit_code" >> "$log_file"
        >&2 echo "$(date --rfc-3339=seconds) END job=$job_name code=$exit_code"

        exit $exit_code
    }
    trap on_exit EXIT

    "$@"
}
export -f run_in_cache


fs-dir-cache exec \
    --key-file Cargo.lock
    --key-str "${CARGO_PROFILE-:dev}" \
    --key-file flake.lock \
    -- \
    bash -c 'run_in_cache "$@"' _ "$@"
```

Using just one tool, it's easy to get correct and practical caching including:

* locking (including fallback timeouts)
* evicition
* timeouts
