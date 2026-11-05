// GuitarScore's service worker: the app shell is cached on the first visit so the editor
// opens offline (a file the user opens still comes from their own disk).
//
// Bump CACHE when the build changes; `cargo xtask web` stamps the version into this file.
const CACHE = "guitarscore-__VERSION__";
const SHELL = [
  "./",
  "./index.html",
  "./guitarscore.js",
  "./guitarscore_bg.wasm",
  "./worklet.js",
  "./worklet-polyfills.js",
  "./worklet_synth.js",
  "./worklet_synth_bg.wasm",
  "./manifest.json",
  "./assets/icon.png",
];

self.addEventListener("install", (event) => {
  event.waitUntil(
    caches
      .open(CACHE)
      // A missing optional file must not fail the install.
      .then((cache) => Promise.allSettled(SHELL.map((url) => cache.add(url))))
      .then(() => self.skipWaiting()),
  );
});

self.addEventListener("activate", (event) => {
  event.waitUntil(
    caches
      .keys()
      .then((keys) =>
        Promise.all(keys.filter((key) => key !== CACHE).map((key) => caches.delete(key))),
      )
      .then(() => self.clients.claim()),
  );
});

self.addEventListener("fetch", (event) => {
  const request = event.request;
  if (request.method !== "GET") return;
  const url = new URL(request.url);
  if (url.origin !== self.location.origin) return;

  // The app's own URL goes to the cached shell first, so the editor opens offline. Other
  // pages (for example worklet-check.html) load normally.
  if (request.mode === "navigate") {
    const isApp = url.pathname.endsWith("/") || url.pathname.endsWith("/index.html");
    if (isApp) {
      event.respondWith(
        caches.match("./index.html").then((cached) => cached || fetch(request)),
      );
      return;
    }
  }

  event.respondWith(
    caches.match(request).then((cached) => {
      if (cached) return cached;
      return fetch(request).then((response) => {
        if (response.ok) {
          const copy = response.clone();
          caches.open(CACHE).then((cache) => cache.put(request, copy));
        }
        return response;
      });
    }),
  );
});
