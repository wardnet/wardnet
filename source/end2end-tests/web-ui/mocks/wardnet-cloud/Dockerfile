# wardnet_cloud_mock image. Build context is this directory. Mirrors
# ../../daemon/mocks/nordvpn/Dockerfile.
FROM node:22-slim@sha256:813a7480f28fdadac1f7f5c824bcdad435b5bc1322a5968bbbdef8d058f9dff4

WORKDIR /app
COPY server.mjs /app/server.mjs

ENV PORT=8080

EXPOSE 8080
CMD ["node", "/app/server.mjs"]
