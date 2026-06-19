# Build/test image for evident: Rust toolchain + Z3 (z3-sys links system
# libz3; bindgen needs libclang) + libffi for the kernel's trampoline.
FROM rust:1-bookworm

RUN apt-get update && apt-get install -y --no-install-recommends \
    z3 libz3-dev clang libclang-dev libffi-dev pkg-config cmake \
    python3 git curl ca-certificates jq ripgrep procps \
 && rm -rf /var/lib/apt/lists/*

# The repo's .cargo/config.toml pins Z3_SYS_Z3_HEADER (and friends) to macOS
# homebrew paths. cargo's [env] does not override variables that are already
# set in the environment, so these take precedence inside the container.
ENV Z3_SYS_Z3_HEADER=/usr/include/z3.h \
    PKG_CONFIG_PATH= \
    LIBRARY_PATH=

WORKDIR /root
CMD ["bash"]
