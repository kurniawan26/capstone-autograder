# syntax=docker/dockerfile:1

ARG PLAYWRIGHT_VERSION=1.60.0

FROM golang:1.25-bookworm AS build

WORKDIR /src
COPY go.mod go.sum ./
RUN go mod download
COPY . .
RUN CGO_ENABLED=1 go build -trimpath -ldflags='-s -w' -o /out/worker ./cmd/worker

FROM debian:bookworm-slim AS railpack

ARG RAILPACK_VERSION=v0.36.0
ARG TARGETARCH=amd64

RUN apt-get update \
 && apt-get install -y --no-install-recommends ca-certificates curl \
 && rm -rf /var/lib/apt/lists/*

RUN set -eux; \
    case "${TARGETARCH}" in \
      amd64) arch=x86_64 ;; \
      arm64) arch=arm64 ;; \
      *) echo "railpack has no release for ${TARGETARCH}" >&2; exit 1 ;; \
    esac; \
    curl -fsSL -o /tmp/railpack.tar.gz \
      "https://github.com/railwayapp/railpack/releases/download/${RAILPACK_VERSION}/railpack-${RAILPACK_VERSION}-${arch}-unknown-linux-musl.tar.gz"; \
    tar -xzf /tmp/railpack.tar.gz -C /tmp; \
    install -D -m 0755 /tmp/railpack /out/railpack

FROM mcr.microsoft.com/playwright:v${PLAYWRIGHT_VERSION}-noble AS runtime

ARG PLAYWRIGHT_VERSION

RUN apt-get update \
 && apt-get install -y --no-install-recommends \
      ca-certificates libwebp7 libwebpdemux2 \
 && rm -rf /var/lib/apt/lists/*

RUN set -eux; \
    libdir="$(dirname "$(ls /usr/lib/*/libwebp.so.7 | head -1)")"; \
    ln -sf libwebp.so.7 "${libdir}/libwebp.so"; \
    ln -sf libwebpdemux.so.2 "${libdir}/libwebpdemux.so"; \
    ldconfig; \
    ldconfig -p | grep -E 'libwebp(demux)?\.so '

COPY --from=docker:cli /usr/local/bin/docker /usr/local/bin/docker
COPY --from=railpack /out/railpack /usr/local/bin/railpack

ENV HOME=/root
COPY scripts/install-playwright-driver.sh /tmp/install-playwright-driver.sh
RUN set -eux; \
    DRIVER_VERSION="${PLAYWRIGHT_VERSION}" /tmp/install-playwright-driver.sh; \
    dest="${HOME}/.cache/ms-playwright-go/${PLAYWRIGHT_VERSION}"; \
    rev="$(node -p "require('${dest}/package/browsers.json').browsers.find(b => b.name === 'chromium').revision")"; \
    if [ ! -d "${PLAYWRIGHT_BROWSERS_PATH}/chromium-${rev}" ]; then \
      echo "driver ${PLAYWRIGHT_VERSION} minta chromium-${rev}, tidak ada di ${PLAYWRIGHT_BROWSERS_PATH}" >&2; \
      ls "${PLAYWRIGHT_BROWSERS_PATH}" >&2; \
      exit 1; \
    fi; \
    echo "driver ${PLAYWRIGHT_VERSION} cocok dengan chromium-${rev} milik image dasar"; \
    rm -rf /tmp/install-playwright-driver.sh /root/.npm

COPY --from=build /out/worker /usr/local/bin/worker

EXPOSE 8090

HEALTHCHECK --interval=30s --timeout=5s --start-period=10s --retries=3 \
  CMD node -e "fetch('http://127.0.0.1:'+(process.env.SANDBOX_WORKER_PORT||8090)+'/healthz').then(r=>process.exit(r.ok?0:1)).catch(()=>process.exit(1))"

ENTRYPOINT ["/usr/local/bin/worker"]
