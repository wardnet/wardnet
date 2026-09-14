# nordvpn_mock image. Build context is this directory. Fixtures are NOT copied
# in — they are bind-mounted read-only at runtime (see compose.yaml) so the
# JSON can be edited without an image rebuild.
FROM node:26-slim@sha256:14bf3eac4bf209d906d3c41256597d3ab1f926b2e93a79e9bdfe1efd32454239

WORKDIR /app
COPY server.mjs /app/server.mjs

ENV PORT=8080 \
    FIXTURES_DIR=/fixtures

EXPOSE 8080
CMD ["node", "/app/server.mjs"]
