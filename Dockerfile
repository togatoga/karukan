FROM ubuntu:24.04

ENV DEBIAN_FRONTEND=noninteractive

# Phase 1: Install dependencies (individual packages, no build-essential)
RUN apt-get update && apt-get install -y --no-install-recommends \
    # C/C++ toolchain (build-essential alternative)
    gcc g++ make \
    # Library detection (openssl-sys, cmake)
    pkg-config \
    # bindgen (llama-cpp-sys-2)
    clang libclang-dev \
    # fcitx5 addon build
    cmake extra-cmake-modules \
    # fcitx5 development headers
    libfcitx5core-dev libfcitx5config-dev libfcitx5utils-dev fcitx5-modules-dev \
    # fcitx5 runtime (addon load verification)
    fcitx5 \
    # D-Bus session (dbus-run-session for headless fcitx5 verification)
    dbus \
    # xkbcommon
    libxkbcommon-dev \
    # OpenSSL (hf-hub)
    libssl-dev \
    # rustup, cargo
    curl ca-certificates git \
    && rm -rf /var/lib/apt/lists/*

# Phase 2: Install Rust toolchain
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --default-toolchain stable
ENV PATH="/root/.cargo/bin:${PATH}"

WORKDIR /workspace
COPY . .

# Phase 3: Build all workspace crates
RUN cargo build --release

# Phase 4: Run karukan-im tests
RUN cargo test -p karukan-im

# Phase 5: Build and install fcitx5 addon (system install)
RUN cd karukan-im/fcitx5-addon \
    && cmake -B build -DCMAKE_INSTALL_PREFIX=/usr \
    && cmake --build build -j"$(nproc)" \
    && cmake --install build

# Phase 6: Verification (3 stages)

# 6a: File existence check
RUN FCITX5_ADDON_DIR=$(pkg-config --variable=libdir Fcitx5Core)/fcitx5 \
    && echo "=== Verifying installed files ===" \
    && echo "Addon dir: ${FCITX5_ADDON_DIR}" \
    && test -f "${FCITX5_ADDON_DIR}/karukan.so" \
    && echo "OK: karukan.so" \
    && test -f "${FCITX5_ADDON_DIR}/libkarukan_im.so" \
    && echo "OK: libkarukan_im.so" \
    && test -f /usr/share/fcitx5/addon/karukan.conf \
    && echo "OK: addon/karukan.conf" \
    && test -f /usr/share/fcitx5/inputmethod/karukan.conf \
    && echo "OK: inputmethod/karukan.conf" \
    && echo "All files verified."

# 6b: ldd check — ensure no missing shared library dependencies
RUN FCITX5_ADDON_DIR=$(pkg-config --variable=libdir Fcitx5Core)/fcitx5 \
    && echo "=== Checking shared library dependencies ===" \
    && ldd "${FCITX5_ADDON_DIR}/karukan.so" \
    && ! ldd "${FCITX5_ADDON_DIR}/karukan.so" | grep "not found" \
    && echo "All shared library dependencies resolved."

# 6c: fcitx5 addon load verification (headless with dbus session)
RUN echo "=== Verifying fcitx5 addon loading ===" \
    && FCITX5_LOG=$(dbus-run-session -- timeout 20 fcitx5 2>&1 || true) \
    && echo "$FCITX5_LOG" \
    && echo "$FCITX5_LOG" | grep -q "Loaded addon karukan" \
    && echo "Addon load verified successfully."

CMD ["echo", "All integration tests passed."]
