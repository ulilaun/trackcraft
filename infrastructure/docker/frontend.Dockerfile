# TrackCraft-Frontend für die Docker-Plattform (RFC-0005): statischer Build hinter Nginx, ohne root.
# Build-Kontext: Repository-Wurzel.
FROM node:24-alpine AS build
WORKDIR /src
COPY frontend/package.json frontend/package-lock.json ./
RUN npm ci
COPY frontend/ ./
RUN npm run build

FROM nginxinc/nginx-unprivileged:stable-alpine
COPY infrastructure/docker/nginx.conf /etc/nginx/conf.d/default.conf
COPY --from=build /src/dist /usr/share/nginx/html
EXPOSE 8080
