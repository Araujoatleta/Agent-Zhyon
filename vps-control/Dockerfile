FROM node:22-alpine
WORKDIR /app
COPY package*.json ./
RUN npm ci --omit=dev
COPY src ./src
RUN chmod -R u=rwX,go=rX /app \
    && install -d -o node -g node -m 0700 /data
USER node
ENV PORT=8790 DATA_FILE=/data/control.json
VOLUME /data
EXPOSE 8790
CMD ["npm","start"]
