# Two stages: build statically against musl, then ship only the binary.
# The runtime image has no shell and no curl on purpose, which is why the
# healthcheck asks the binary itself (AR13, kyu's T9 pattern).
FROM rust:1.97-alpine AS builder
RUN apk add --no-cache musl-dev
WORKDIR /src
COPY . .
RUN cargo build --release --target x86_64-unknown-linux-musl --locked

FROM gcr.io/distroless/static-debian12:nonroot
COPY --from=builder /src/target/x86_64-unknown-linux-musl/release/http-switchboard /usr/local/bin/http-switchboard
# CA certificates come with the distroless image, so an https destination
# outside the house works (scope K3/NG5).
USER nonroot:nonroot
EXPOSE 8080
HEALTHCHECK --interval=30s --timeout=5s --start-period=5s --retries=3 \
  CMD ["/usr/local/bin/http-switchboard", "--healthcheck", "http://127.0.0.1:8080/healthz"]
ENTRYPOINT ["/usr/local/bin/http-switchboard"]
CMD ["/etc/http-switchboard/config.toml"]
