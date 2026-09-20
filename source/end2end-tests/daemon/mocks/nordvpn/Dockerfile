# nordvpn_mock image. Build context is this directory. Fixtures are NOT copied
# in — they are bind-mounted read-only at runtime (see compose.yaml) so the
# JSON can be edited without an image rebuild.
FROM node:26-slim@sha256:c0753125a3789977aefe869cbebccf70e3cfd7ea84ca48547458f02e4f1d7146

WORKDIR /app
COPY server.mjs /app/server.mjs

ENV PORT=8080 \
    FIXTURES_DIR=/fixtures

EXPOSE 8080
CMD ["node", "/app/server.mjs"]
