FROM node:24-alpine

WORKDIR /app

COPY package.json package-lock.json ./
RUN apk add --no-cache su-exec \
    && npm ci --omit=dev --ignore-scripts \
    && npm cache clean --force

COPY src ./src
COPY tools/dev-relay.ts tools/vps-relay.ts ./tools/
COPY deploy/docker-entrypoint.sh /usr/local/bin/coucou-relay-entrypoint

RUN chmod 0555 /usr/local/bin/coucou-relay-entrypoint

ENV HOST=0.0.0.0 \
    PORT=8787 \
    CONNECTS_PER_MINUTE=30 \
    ACCESS_KEY_FILE=/run/secrets/coucou_relay_access

EXPOSE 8787

HEALTHCHECK --interval=30s --timeout=3s --start-period=5s --retries=3 \
  CMD node -e "fetch('http://127.0.0.1:8787/').then(r=>{if(!r.ok)process.exit(1)}).catch(()=>process.exit(1))"

ENTRYPOINT ["coucou-relay-entrypoint"]
CMD ["node", "tools/vps-relay.ts"]
