# FS Dir Cache

A CLI tool for CIs and build scripts, making file system based
caching easy and correct (locking, eviction, etc.)

When working on build systems / CIs it's often a requirement to
utilize some best effort caching, with locking and eviction.

Not exactly rocket science, but non-trivial enough to not want
to implement and maintain ad-hoc.

`fs-dir-cache` aims to be a simple to use utility from inside
other scripts and programs taking care of the details.

## Use case

### CI cache

Imagine that you have a CI runner that can persist files between runs,
and you'd like to utilize it to reuse and speed up some things:


```bash
set -euo pipefail
 
FS_DIR_CACHE_ROOT="$HOME/.cache/fs-dir-cache" # directory to hold all cache (sub)directories
FS_DIR_CACHE_LOCK_ID="pid-$$-rnd-$RANDOM"     # acquire lock based on the current pid and something random (just in case pid gets reused)
FS_DIR_CACHE_KEY_NAME="build-project-x"       # the base name of our key
FS_DIR_CACHE_LOCK_TIMEOUT_SECS="600"      # unlock after timeout in case our job fails misereably

fs-dir-cache gc unused --seconds "$((7 * 24 * 60 * 60))" # delete caches not used in more than a week

# create/reuse cache (sub-directory) and lock it (wait if already locked)
cache_dir=$(fs-dir-cache lock --key-file Cargo.toml)
# unlock it when the script finish
trap "fs-dir-cache unlock --dir ${cache_dir}" EXIT

# 'cache_dir' will now equal to something like '/home/user/.cache/fs-dir-cache/build-project-x-8jg9hsadjfkaj9jkfljdfsd'
# and script has up to 600s to use it exclusively

# build project
cargo build --target-dir="${cache_dir}/target"
```

Using just one tool, it's easy to get correct and practical caching including:

* locking (including fallback timeouts)
* evicition
* timeouts
