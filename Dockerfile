# Two stages on the same Debian the LXCs run (T8): a glibc binary that
# also works copied out of the image. The runtime stage has no shell
# tools, so the container HEALTHCHECK uses the binary's own --healthcheck.
FROM rust:1.97-slim-trixie AS build
RUN apt-get update -qq && apt-get install -y -qq --no-install-recommends pkg-config libssl-dev && rm -rf /var/lib/apt/lists/*
WORKDIR /src
COPY . .
RUN cargo build --release --locked

FROM debian:trixie-slim
RUN apt-get update -qq && apt-get install -y -qq --no-install-recommends ca-certificates libssl3t64 && rm -rf /var/lib/apt/lists/* \
    && useradd --system --uid 10001 --home /var/lib/http-switchboard --shell /usr/sbin/nologin http-switchboard \
    && mkdir -p /var/lib/http-switchboard && chown http-switchboard:http-switchboard /var/lib/http-switchboard
COPY --from=build /src/target/release/http-switchboard /usr/local/bin/http-switchboard
USER http-switchboard
ENV HTTP_SWITCHBOARD_LISTEN=0.0.0.0:8080 HTTP_SWITCHBOARD_STATE_DIR=/var/lib/http-switchboard
EXPOSE 8080
VOLUME ["/var/lib/http-switchboard"]
# Self-update is off inside an image by detection (AR8); updates are a new image.
HEALTHCHECK --interval=30s --timeout=5s --retries=3 CMD ["/usr/local/bin/http-switchboard", "--healthcheck"]
ENTRYPOINT ["/usr/local/bin/http-switchboard"]
