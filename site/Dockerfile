FROM node:24-alpine AS build
WORKDIR /app
COPY package.json package-lock.json ./
COPY site/package.json site/package-lock.json ./site/
RUN npm ci --ignore-scripts && npm ci --prefix site
COPY scripts/ ./scripts/
COPY catalog/ ./catalog/
COPY docs/ ./docs/
COPY site/.vitepress/ ./site/.vitepress/
COPY ui/public/ ./ui/public/
COPY ui/src/components/AppIcon.vue ./ui/src/components/AppIcon.vue
COPY packages/selfhost/package.json ./packages/selfhost/package.json
RUN npm run build --prefix site

FROM caddy:2-alpine AS runtime
COPY site/Caddyfile /etc/caddy/Caddyfile
COPY --from=build /app/site/.vitepress/dist /srv
USER 10001:10001
EXPOSE 8080
HEALTHCHECK --interval=30s --timeout=5s --start-period=10s --retries=3 \
  CMD wget -q -O /dev/null http://127.0.0.1:8080/ || exit 1
CMD ["caddy", "run", "--config", "/etc/caddy/Caddyfile", "--adapter", "caddyfile"]
