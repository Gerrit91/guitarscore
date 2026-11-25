// GuitarScore's service worker: the app shell is cached on the first visit so the editor
// opens offline (a file the user opens still comes from their own disk).
//
// SoundFonts have a cache of their own. They are large (about 5 MB compressed), and their names
// carry the version, so a new build must not throw the old one away on every deploy: it
// would download again for nothing. The soundfont cache survives app-cache changes and is
// pruned to the manifest's current file when a new worker activates.
//
// CACHE is unique to each build: `cargo xtask web` stamps the version and the build time
// into this file, so every build is a new worker and a new cache. A new worker waits after
// installing until the page tells it to take over (`skipWaiting` message): at once if the
// page is untouched, otherwise when the user presses Reload in its update bar
// (index.html). The page reloads once when the new worker takes over.
//
// Two builds may share the origin: the release at the site's root and a preview under
// `preview/` (`.github/workflows/preview.yml`). Each worker keeps to its own scope: its cache
// names carry the scope, it deletes only its own old caches, and the root's worker leaves
// the preview's requests alone (the browser hands them to the preview's own worker, whose
// scope is the more specific one).
const SCOPE = new URL(self.registration.scope).pathname;
const PREFIX = `guitarscore-${SCOPE}-`;
const CACHE = `${PREFIX}__VERSION__`;
const SOUNDFONT_CACHE = `guitarscore-soundfonts-${SCOPE}`;
const PREVIEW = `${SCOPE}preview/`;
const SOUNDFONT_DIR = "./assets/soundfonts/";
const SOUNDFONT_MANIFEST = "./assets/soundfonts/soundfont.json";
const SHELL = [
  "./",
  "./index.html",
  "./guitarscore.js",
  "./guitarscore_bg.wasm",
  "./worklet.js",
  "./worklet-polyfills.js",
  "./recovery.js",
  "./worklet_synth.js",
  "./worklet_synth_bg.wasm",
  "./manifest.json",
  "./assets/icon.png",
  // The SoundFont manifest belongs to the build: an older app must not be pointed at a
  // newer font it may not read (a cached app got the compressed SF3, 2026-11-20).
  "./assets/soundfonts/soundfont.json",
];

self.addEventListener("install", (event) => {
  event.waitUntil(
    caches
      .open(CACHE)
      // A missing optional file must not fail the install. The SoundFont is not here: the
      // install would wait for 30 MB.
      // `reload` skips the browser's HTTP cache: otherwise a new build's cache could be
      // filled with the previous build's files (seen in testing, 2026-11-13).
      .then((cache) =>
        Promise.allSettled(SHELL.map((url) => cache.add(new Request(url, { cache: "reload" })))),
      ),
  );
});

// The page decides when a waiting new build takes over (see the top of this file).
self.addEventListener("message", (event) => {
  if (event.data && event.data.type === "skipWaiting") {
    self.skipWaiting();
  }
});

self.addEventListener("activate", (event) => {
  event.waitUntil(
    (async () => {
      const keys = await caches.keys();
      await Promise.all(
        keys
          // Only this scope's own older builds. The caches of builds from before the
          // scope was part of the name ("guitarscore-<version>", "guitarscore-soundfonts")
          // went with the root's worker.
          .filter(
            (key) =>
              key !== CACHE &&
              key !== SOUNDFONT_CACHE &&
              (key.startsWith(PREFIX) ||
                (SCOPE !== PREVIEW_ROOT && isUnscoped(key))),
          )
          .map((key) => caches.delete(key)),
      );
      await pruneSoundfonts();
      await self.clients.claim();
    })(),
  );
});

/// The preview's own scope, seen from inside it: `/<site>/preview/`. Its worker must not
/// clean up after the root's.
const PREVIEW_ROOT = SCOPE.endsWith("/preview/") ? SCOPE : null;

/// A cache name from before the scope was part of it (the root's worker cleans them up).
function isUnscoped(key) {
  return key === "guitarscore-soundfonts" || /^guitarscore-[^/]*$/.test(key);
}

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
  // A preview under this scope has a worker of its own.
  if (url.pathname.startsWith(PREVIEW)) return;

  // The app's own URL goes to the cached shell first, so the editor opens offline. Other
  // pages (for example worklet-check.html) load normally.
  if (request.mode === "navigate") {
    // Only this scope's own page, not a folder below it.
    const isApp = url.pathname === SCOPE || url.pathname === `${SCOPE}index.html`;
    if (isApp) {
      event.respondWith(
        caches
          .match("./index.html", { cacheName: CACHE })
          .then((cached) => cached || fetch(request)),
      );
      return;
    }
  }

  // The manifest comes from this worker's own build cache, never another build's: the app
  // this worker serves and the font it names must match. A new deploy's font arrives with
  // the new worker (its install caches the new manifest), which reloads the page.
  if (url.pathname.endsWith("/assets/soundfonts/soundfont.json")) {
    event.respondWith(
      (async () => {
        const cache = await caches.open(CACHE);
        const cached = await cache.match(request);
        if (cached) return cached;
        const response = await fetch(request, { cache: "no-cache" });
        if (response.status === 200) {
          await cache.put(request, response.clone());
        }
        return response;
      })(),
    );
    return;
  }

  // A SoundFont is cache-first in its own cache. `cache.put` rejects partial (206)
  // responses, so only a full 200 is stored.
  if (url.pathname.endsWith(".sf2") || url.pathname.endsWith(".sf3")) {
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

  // Only this build's cache: another build's files must never answer for this one.
  event.respondWith(
    caches.match(request, { cacheName: CACHE }).then((cached) => {
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
