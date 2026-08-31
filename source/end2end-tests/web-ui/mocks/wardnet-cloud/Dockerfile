# wardnet_cloud_mock image. Build context is this directory. Mirrors
# ../../daemon/mocks/nordvpn/Dockerfile.
FROM node:26-slim@sha256:c0753125a3789977aefe869cbebccf70e3cfd7ea84ca48547458f02e4f1d7146

WORKDIR /app
COPY server.mjs /app/server.mjs

ENV PORT=8080

EXPOSE 8080
CMD ["node", "/app/server.mjs"]
