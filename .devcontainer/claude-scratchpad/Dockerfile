FROM debian:bookworm-slim

RUN apt-get update \
    && apt-get install -y --no-install-recommends bash ca-certificates curl \
    && rm -rf /var/lib/apt/lists/*

# Keep the probe independent of the host's uid and macOS keychain.
RUN useradd --create-home --uid 1000 --shell /bin/bash probe
USER probe
ENV PATH="/home/probe/.local/bin:${PATH}" \
    DISABLE_AUTOUPDATER=1

# Pin the version used on the host when ADR 26.9.26a was investigated.
RUN curl -fsSL https://claude.ai/install.sh -o /tmp/claude-install.sh \
    && bash /tmp/claude-install.sh 2.1.280 \
    && rm /tmp/claude-install.sh \
    && claude --version

WORKDIR /home/probe/work
