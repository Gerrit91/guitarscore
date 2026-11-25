// The browser's recovery store: the editor's rescue copies (a Guitar Pro 7 copy of every
// song with unsaved edits, `crates/gscore-app/src/app/web_rescue.rs`), kept in memory for
// the crash screen and in IndexedDB for the next visit, so a closed tab, a reload or a
// crash does not lose them.
//
// Every page is a session with a random id; a copy is stored under "<session>/<tab>". A
// live session holds a Web Lock named after it, so a page opened beside it (a second tab
// of the site) does not take its copies for a crashed session's. At the start the editor
// is offered the copies of sessions that are gone (`found`), newest first.
//
// Without IndexedDB (a private window may refuse it) the copies stay in memory only, as
// before. Without Web Locks (browsers before 2022) every other session's copies count as
// gone, even those of a page still open beside this one.

const DATABASE = "guitarscore-recovery";
const STORE = "copies";
const LOCK_PREFIX = "guitarscore-session-";

const randomId = () => {
  try {
    return crypto.randomUUID();
  } catch {
    return `${Date.now().toString(36)}-${Math.random().toString(36).slice(2)}`;
  }
};

// One connection for the page, or null without IndexedDB.
const openDatabase = () =>
  new Promise((resolve) => {
    try {
      const request = indexedDB.open(DATABASE, 1);
      request.onupgradeneeded = () => request.result.createObjectStore(STORE);
      request.onsuccess = () => resolve(request.result);
      request.onerror = () => resolve(null);
      request.onblocked = () => resolve(null);
    } catch {
      resolve(null);
    }
  });

// Runs `work(store)` in one transaction; resolves when it is written (true) or failed.
const write = async (database, work) => {
  const db = await database;
  if (!db) return false;
  return new Promise((resolve) => {
    try {
      const transaction = db.transaction(STORE, "readwrite");
      transaction.oncomplete = () => resolve(true);
      transaction.onerror = () => resolve(false);
      transaction.onabort = () => resolve(false);
      work(transaction.objectStore(STORE));
    } catch {
      resolve(false);
    }
  });
};

// Every stored copy, as [key, value] pairs; [] when nothing can be read.
const readAll = async (database) => {
  const db = await database;
  if (!db) return [];
  return new Promise((resolve) => {
    try {
      const store = db.transaction(STORE, "readonly").objectStore(STORE);
      const keys = store.getAllKeys();
      const values = store.getAll();
      values.onsuccess = () =>
        resolve((keys.result ?? []).map((key, index) => [key, values.result[index]]));
      values.onerror = () => resolve([]);
    } catch {
      resolve([]);
    }
  });
};

// The sessions whose page is open now (holding their lock), or null without Web Locks.
const liveSessions = async () => {
  try {
    if (!navigator.locks?.query) return null;
    const { held = [], pending = [] } = await navigator.locks.query();
    return new Set(
      [...held, ...pending]
        .map((lock) => lock.name ?? "")
        .filter((name) => name.startsWith(LOCK_PREFIX))
        .map((name) => name.slice(LOCK_PREFIX.length)),
    );
  } catch {
    return null;
  }
};

const isCopy = (value) =>
  value !== null &&
  typeof value === "object" &&
  typeof value.session === "string" &&
  typeof value.name === "string" &&
  value.bytes instanceof Uint8Array &&
  Number.isFinite(value.saved);

/// Sets up `globalThis.guitarscoreRescue` for this page.
export function startRecovery() {
  const session = randomId();
  // This page's own copies, by tab key, for the crash screen.
  const copies = new Map();
  // The lock lives as long as the page; the browser lets it go when the page goes.
  try {
    navigator.locks?.request(LOCK_PREFIX + session, () => new Promise(() => {}));
  } catch {}
  const database = openDatabase();
  const keyOf = (tab) => `${session}/${tab}`;

  const rescue = {
    session,
    // The copies of sessions that are gone, newest first: null while they are read.
    found: null,
    // This page's copies, for the crash screen.
    copies: () => [...copies.values()],
    // The editor's copy of tab `tab`'s song (the bytes Save would download as `name`).
    keep(tab, name, bytes) {
      const copy = { session, name, bytes: new Uint8Array(bytes), saved: Date.now() };
      copies.set(tab, copy);
      write(database, (store) => store.put(copy, keyOf(tab)));
    },
    // Tab `tab` was saved or closed: its copy goes.
    forget(tab) {
      copies.delete(tab);
      write(database, (store) => store.delete(keyOf(tab)));
    },
    // The bytes of found copy `id`, or null.
    bytes: (id) => rescue.found?.find((copy) => copy.id === id)?.bytes ?? null,
    // Found copy `id` was restored into tab `tab`: it is that tab's copy from now on.
    adopt(id, tab) {
      const copy = rescue.found?.find((entry) => entry.id === id);
      rescue.found = rescue.found?.filter((entry) => entry.id !== id) ?? null;
      if (!copy) return;
      const own = { session, name: copy.name, bytes: copy.bytes, saved: copy.saved };
      copies.set(tab, own);
      write(database, (store) => {
        store.delete(id);
        store.put(own, keyOf(tab));
      });
    },
    // Found copy `id` is not wanted.
    discard(id) {
      rescue.found = rescue.found?.filter((entry) => entry.id !== id) ?? null;
      write(database, (store) => store.delete(id));
    },
  };
  globalThis.guitarscoreRescue = rescue;

  (async () => {
    const [entries, live] = await Promise.all([readAll(database), liveSessions()]);
    rescue.found = entries
      .filter(([key, value]) => typeof key === "string" && isCopy(value))
      .filter(([, value]) => value.session !== session && !live?.has(value.session))
      .map(([key, value]) => ({
        id: key,
        name: value.name,
        bytes: value.bytes,
        saved: value.saved,
        when: new Date(value.saved).toLocaleString(),
      }))
      .sort((a, b) => b.saved - a.saved);
  })().catch(() => {
    rescue.found = [];
  });
  return rescue;
}
