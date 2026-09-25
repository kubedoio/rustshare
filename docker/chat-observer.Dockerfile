FROM node:22.22.2-alpine@sha256:8ea2348b068a9544dae7317b4f3aafcdc032df1647bb7d768a05a5cad1a7683f

WORKDIR /app
ENV NODE_ENV=production

COPY frontend/package.json frontend/package-lock.json ./
RUN npm ci --omit=dev --ignore-scripts --no-audit --no-fund

COPY frontend/scripts/buzz-observer.mjs frontend/scripts/chat-bootstrap.mjs \
     frontend/scripts/alpha-buzz-ops.mjs ./scripts/

USER node
EXPOSE 8091
HEALTHCHECK --interval=15s --timeout=3s --start-period=10s --retries=4 \
  CMD wget -q -O - http://127.0.0.1:8091/ready >/dev/null || exit 1

CMD ["node", "scripts/buzz-observer.mjs"]
