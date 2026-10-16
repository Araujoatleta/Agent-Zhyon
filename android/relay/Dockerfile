FROM node:24-alpine

WORKDIR /app

COPY package.json package-lock.json ./
RUN npm ci --omit=dev --ignore-scripts && npm cache clean --force

COPY src ./src
COPY tools/dev-relay.ts tools/vps-relay.ts tools/smoke-relay.ts ./tools/

ENV HOST=0.0.0.0 \
    PORT=8787 \
    CONNECTS_PER_MINUTE=30 \
    ACCESS_KEYS_DIR=/run/coucou/clients

USER node
EXPOSE 8787

HEALTHCHECK --interval=30s --timeout=3s --start-period=5s --retries=3 \
  CMD node -e "fetch('http://127.0.0.1:8787/').then(r=>{if(!r.ok)process.exit(1)}).catch(()=>process.exit(1))"

CMD ["node", "tools/vps-relay.ts"]
