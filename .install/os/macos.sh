#!/usr/bin/env bash
# macOS (homebrew). Conditional python@3.11; PATH for GNU make, llvm@18, coreutils.

. "$LIB/packages.sh"

if ! command -v brew >/dev/null 2>&1; then
    echo "macos.sh: brew is not installed; install from https://brew.sh" >&2
    exit 1
fi

brew_default_install

if ! command -v python3 >/dev/null 2>&1; then
    HOMEBREW_NO_AUTO_UPDATE=1 _run brew install python@3.11
fi

LLVM_VERSION="18"
BREW_PREFIX="$(brew --prefix)"
GNUBIN="$BREW_PREFIX/opt/make/libexec/gnubin"
LLVM="$BREW_PREFIX/opt/llvm@$LLVM_VERSION/bin"
COREUTILS="$BREW_PREFIX/opt/coreutils/libexec/gnubin"

update_profile() {
    local profile_path=$1
    local newpath="export PATH=$COREUTILS:$LLVM:$GNUBIN:\$PATH"
    grep -qxF "$newpath" "$profile_path" 2>/dev/null \
        || printf '%s\n' "$newpath" >> "$profile_path"
}

# PATH munging writes to the user's shell profiles / $GITHUB_PATH — mutations.
# Skip entirely in list/dry-run mode: neither may modify anything.
if [ "${CHECK_DEPS:-0}" != 1 ] && [ "${DRY_RUN:-0}" != 1 ]; then
    [ -f "$HOME/.bash_profile" ] && update_profile "$HOME/.bash_profile"
    [ -f "$HOME/.zshrc" ]        && update_profile "$HOME/.zshrc"

    # GitHub Actions: $GITHUB_PATH expects one directory per line (not an export
    # statement). Writing the full `export PATH=...` line would add a single
    # garbage entry to PATH instead of prepending the three directories.
    if [ -n "${GITHUB_PATH:-}" ]; then
        printf '%s\n%s\n%s\n' "$COREUTILS" "$LLVM" "$GNUBIN" >> "$GITHUB_PATH"
    fi
fi
true
