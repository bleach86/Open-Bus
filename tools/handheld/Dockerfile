# What tools/build-handheld builds the emulator for a 64-bit ARM handheld
# with: Debian 13's (trixie's) cross compiler and its arm64 SDL2, the
# same glibc and SDL2 the RG351MP's Debian has; and its own C compiler,
# for the crates' build scripts, which run here. Rust's the host's own,
# mounted in.
FROM debian:trixie-slim
RUN dpkg --add-architecture arm64 \
    && apt-get update \
    && apt-get install -y --no-install-recommends \
        gcc libc6-dev gcc-aarch64-linux-gnu libc6-dev-arm64-cross libsdl2-dev:arm64 \
    && rm -rf /var/lib/apt/lists/*
