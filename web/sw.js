// GuitarScore's service worker: the app shell is cached on the first visit so the editor
// opens offline (a file the user opens still comes from their own disk).
//
// SoundFonts have a cache of their own. They are large (about 30 MB), and their names
// carry the version, so a new build must not throw the old one away on every deploy: it
// would download again for nothing. The soundfont cache survives app-cache changes and is
// pruned to the manifest's current file when a new worker activates.
//
// Bump CACHE when the build changes; `cargo xtask web` stamps the version into this file.
const CACHE = "guitarscore-__VERSION__";
const SOUNDFONT_CACHE = "guitarscore-soundfonts";
const SOUNDFONT_DIR = "./assets/soundfonts/";
const SOUNDFONT_MANIFEST = "./assets/soundfonts/soundfont.json";
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
      // A missing optional file must not fail the install. The SoundFont is not here: the
      // install would wait for 30 MB.
      .then((cache) => Promise.allSettled(SHELL.map((url) => cache.add(url))))
      .then(() => self.skipWaiting()),
  );
});

self.addEventListener("activate", (event) => {
  event.waitUntil(
    (async () => {
      const keys = await caches.keys();
      await Promise.all(
        keys
          .filter((key) => key !== CACHE && key !== SOUNDFONT_CACHE)
          .map((key) => caches.delete(key)),
      );
      await pruneSoundfonts();
      await self.clients.claim();
    })(),
  );
});

/// Keeps only the SoundFont the current manifest names. Offline (no manifest), nothing is
/// deleted: a font fetched for offline use must stay usable.
async function pruneSoundfonts() {
  let current = null;
  try {
    const response = await fetch(SOUNDFONT_MANIFEST, { cache: "no-cache" });
    if (response.ok) {
      const manifest = await response.json();
      if (manifest && typeof manifest.url === "string" && manifest.url.length > 0) {
        current = new URL(manifest.url, new URL(SOUNDFONT_DIR, self.registration.scope)).href;
      }
    }
  } catch {
    return;
  }
  if (!current) return;
  const cache = await caches.open(SOUNDFONT_CACHE);
  const keys = await cache.keys();
  await Promise.all(
    keys.map((request) => (request.url === current ? null : cache.delete(request))),
  );
}

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

  // The manifest is network-first, so a new deploy's font is noticed; the cache answers
  // when offline.
  if (url.pathname.endsWith("/assets/soundfonts/soundfont.json")) {
    event.respondWith(
      (async () => {
        try {
          const response = await fetch(request, { cache: "no-cache" });
          if (response.status === 200) {
            const copy = response.clone();
            caches.open(CACHE).then((cache) => cache.put(request, copy));
          }
          return response;
        } catch {
          return (await caches.match(request)) || Response.error();
        }
      })(),
    );
    return;
  }

  // A SoundFont is cache-first in its own cache. `cache.put` rejects partial (206)
  // responses, so only a full 200 is stored.
  if (url.pathname.endsWith(".sf2")) {
    event.respondWith(
      (async () => {
        const cache = await caches.open(SOUNDFONT_CACHE);
        const cached = await cache.match(request);
        if (cached) return cached;
        const response = await fetch(request);
        if (response.status === 200) {
          await cache.put(request, response.clone());
        }
        return response;
      })(),
    );
    return;
  }

  event.respondWith(
    caches.match(request).then((cached) => {
      if (cached) return cached;
      return fetch(request).then((response) => {
        if (response.status === 200) {
          const copy = response.clone();
          caches.open(CACHE).then((cache) => cache.put(request, copy));
        }
        return response;
      });
    }),
  );
});
