// Offline use (spec 14.1): the site is cached on the first visit and served
// from the cache afterwards, refreshed in the background when online.

const CACHE = "dedalo-v4";
const CORE = [
  "./",
  "index.html",
  "style.css",
  "main.js",
  "editor.js",
  "i18n.js",
  "worker.js",
  "dedalo.js",
  "dedalo.wasm",
  "dedalo-export.wasm",
  "manifest.webmanifest",
  "icon.svg",
  "reference.html",
  "reference.js",
];

self.addEventListener("install", (event) => {
  event.waitUntil(
    caches
      .open(CACHE)
      .then((cache) => Promise.all(CORE.map((url) => cache.add(url).catch(() => {}))))
      .then(() => self.skipWaiting()),
  );
});

self.addEventListener("activate", (event) => {
  event.waitUntil(
    caches
      .keys()
      .then((keys) => Promise.all(keys.filter((k) => k !== CACHE).map((k) => caches.delete(k))))
      .then(() => self.clients.claim()),
  );
});

self.addEventListener("fetch", (event) => {
  const request = event.request;
  if (request.method !== "GET" || new URL(request.url).origin !== location.origin) return;
  event.respondWith(
    caches.open(CACHE).then(async (cache) => {
      const cached = await cache.match(request, { ignoreSearch: true });
      const fresh = fetch(request)
        .then((response) => {
          if (response.ok) cache.put(request, response.clone());
          return response;
        })
        .catch(() => cached);
      return cached || fresh;
    }),
  );
});
