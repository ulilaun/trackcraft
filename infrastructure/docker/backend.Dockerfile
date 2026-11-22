# TrackCraft-Backend für die Docker-Plattform (RFC-0005), nur linux/amd64.
# Build-Kontext: Repository-Wurzel.
FROM rust:1-bookworm AS build
ARG TRACKCRAFT_VERSION=""
ARG TRACKCRAFT_COMMIT=""
ENV TRACKCRAFT_VERSION=${TRACKCRAFT_VERSION} TRACKCRAFT_COMMIT=${TRACKCRAFT_COMMIT}
WORKDIR /src
COPY backend/ ./
RUN cargo build --release --locked && strip target/release/trackcraft

FROM debian:bookworm-slim
# Kein root (RFC-0011); Datenordner gehören dem Benutzer 10001.
RUN useradd --system --uid 10001 --no-create-home trackcraft \
    && mkdir -p /data /config /backup-target \
    && chown 10001:10001 /data /backup-target
COPY --from=build /src/target/release/trackcraft /usr/local/bin/trackcraft
USER 10001
EXPOSE 8080
VOLUME ["/data"]
ENTRYPOINT ["/usr/local/bin/trackcraft"]
