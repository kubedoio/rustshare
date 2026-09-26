FROM node:22-alpine@sha256:0a7108bf6c7bf5de370ffb1a3ed6be93d405b43ff159f681a8d18c0e2bc2e402

WORKDIR /app
ENV NODE_ENV=production

COPY docker/chat-observer.package.json ./package.json
COPY docker/chat-observer.package-lock.json ./package-lock.json
RUN npm ci --omit=dev --ignore-scripts --no-audit --no-fund \
    && npm cache clean --force \
    && rm -rf /usr/local/lib/node_modules/npm /usr/local/bin/npm /usr/local/bin/npx

COPY frontend/scripts/buzz-observer.mjs frontend/scripts/chat-bootstrap.mjs \
     frontend/scripts/alpha-buzz-ops.mjs ./scripts/

USER node
EXPOSE 8091
HEALTHCHECK --interval=15s --timeout=3s --start-period=10s --retries=4 \
  CMD wget -q -O - http://127.0.0.1:8091/ready >/dev/null || exit 1

CMD ["node", "scripts/buzz-observer.mjs"]
