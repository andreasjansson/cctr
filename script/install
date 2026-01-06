#!/usr/bin/env bash
set -euo pipefail

BINARY_NAME="cctr"
DEFAULT_INSTALL_DIR="${HOME}/.local/bin"

usage() {
    cat <<EOF
Install cctr - CLI Corpus Test Runner

USAGE:
    ./install.sh [OPTIONS]

OPTIONS:
    -d, --dir <DIR>     Installation directory [default: ~/.local/bin]
    -s, --system        Install to /usr/local/bin (requires sudo)
    -h, --help          Show this help message

EXAMPLES:
    ./install.sh                    # Install to ~/.local/bin
    ./install.sh -d ~/bin           # Install to ~/bin
    ./install.sh --system           # Install to /usr/local/bin
EOF
}

main() {
    local install_dir="$DEFAULT_INSTALL_DIR"
    local use_sudo=false

    while [[ $# -gt 0 ]]; do
        case "$1" in
            -d|--dir)
                install_dir="$2"
                shift 2
                ;;
            -s|--system)
                install_dir="/usr/local/bin"
                use_sudo=true
                shift
                ;;
            -h|--help)
                usage
                exit 0
                ;;
            *)
                echo "Unknown option: $1" >&2
                usage
                exit 1
                ;;
        esac
    done

    if ! command -v cargo &> /dev/null; then
        echo "Error: cargo not found. Please install Rust: https://rustup.rs" >&2
        exit 1
    fi

    echo "Building cctr (release mode)..."
    cargo build --release --package cctr

    mkdir -p "$install_dir"

    local src="target/release/${BINARY_NAME}"
    local dst="${install_dir}/${BINARY_NAME}"

    echo "Installing to ${dst}..."
    if [[ "$use_sudo" == true ]]; then
        sudo cp "$src" "$dst"
        sudo chmod +x "$dst"
    else
        cp "$src" "$dst"
        chmod +x "$dst"
    fi

    echo ""
    echo "✓ cctr installed successfully to ${dst}"
    
    if ! echo "$PATH" | tr ':' '\n' | grep -qx "$install_dir"; then
        echo ""
        echo "Note: ${install_dir} is not in your PATH."
        echo "Add it with:"
        echo "    export PATH=\"${install_dir}:\$PATH\""
    fi
}

main "$@"
