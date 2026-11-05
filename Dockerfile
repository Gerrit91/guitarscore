# The browser build served by nginx. The site is built beforehand (`cargo xtask web`, done
# by the release workflow); this image only copies it in:
#
#   cargo xtask web && docker build -t guitarscore-web .
#   docker run --rm -p 8080:80 guitarscore-web     # http://localhost:8080
FROM nginx:1.29-alpine
COPY docker/nginx.conf /etc/nginx/conf.d/default.conf
COPY dist/web /usr/share/nginx/html
