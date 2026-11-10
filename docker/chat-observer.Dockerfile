FROM node:26-alpine@sha256:8996bb5821d62a8fd9bb27c1cb07462f3fdce6e47d640c6a881ead0b064ce216

WORKDIR /app
ENV NODE_ENV=production

COPY docker/chat-observer.package.json ./package.json
COPY docker/chat-observer.package-lock.json ./package-lock.json
RUN npm ci --omit=dev --ignore-scripts --no-audit --no-fund \
    && npm cache clean --force \
    && rm -rf /usr/local/lib/node_modules/npm /usr/local/bin/npm /usr/local/bin/npx

COPY frontend/scripts/buzz-observer.mjs frontend/scripts/buzz-observer-policy.mjs frontend/scripts/chat-bootstrap.mjs \
     frontend/scripts/alpha-buzz-ops.mjs ./scripts/

USER node
EXPOSE 8091
HEALTHCHECK --interval=15s --timeout=3s --start-period=10s --retries=4 \
  CMD wget -q -O - http://127.0.0.1:8091/ready >/dev/null || exit 1

CMD ["node", "scripts/buzz-observer.mjs"]
