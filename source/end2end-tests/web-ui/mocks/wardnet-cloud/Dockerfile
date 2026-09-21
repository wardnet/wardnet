# wardnet_cloud_mock image. Build context is this directory. Mirrors
# ../../daemon/mocks/nordvpn/Dockerfile.
FROM node:26-slim@sha256:14bf3eac4bf209d906d3c41256597d3ab1f926b2e93a79e9bdfe1efd32454239

WORKDIR /app
COPY server.mjs /app/server.mjs

ENV PORT=8080

EXPOSE 8080
CMD ["node", "/app/server.mjs"]
