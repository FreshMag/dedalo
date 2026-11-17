// Loads Dedalo WebAssembly modules and returns a synchronous `call`.
//
// The modules expose three raw exports: `dedalo_alloc`, `dedalo_free` and
// `dedalo_call`. Requests and responses are JSON. `loadCore` also hosts
// plugins (spec 11.5): the core calls them through the `dedalo_host`
// import, and this file runs each one with the browser's own engine and the
// same four `dedalo.*` imports the binary provides. It runs unchanged in
// browsers and in Node.

const encoder = new TextEncoder();
const decoder = new TextDecoder();

async function bytesOf(source) {
  if (typeof source === "string" || source instanceof URL) {
    const response = await fetch(source);
    if (!response.ok) throw new Error(`cannot load ${source}: HTTP ${response.status}`);
    return response.arrayBuffer();
  }
  return source;
}

function packedCall(exports, request) {
  const input = request instanceof Uint8Array ? request : encoder.encode(typeof request === "string" ? request : JSON.stringify(request));
  const inputPtr = exports.dedalo_alloc(input.length);
  new Uint8Array(exports.memory.buffer, inputPtr, input.length).set(input);
  const packed = BigInt.asUintN(64, BigInt(exports.dedalo_call(inputPtr, input.length)));
  exports.dedalo_free(inputPtr, input.length);
  const ptr = Number(packed >> 32n);
  const len = Number(packed & 0xffffffffn);
  const bytes = new Uint8Array(exports.memory.buffer, ptr, len).slice();
  exports.dedalo_free(ptr, len);
  return bytes;
}

/**
 * Loads a module that needs no host (the export module).
 * @returns {Promise<(request: object) => any>}
 */
export async function loadDedalo(source) {
  const { instance } = await WebAssembly.instantiate(await bytesOf(source), {});
  return (request) => JSON.parse(decoder.decode(packedCall(instance.exports, request)));
}

// SplitMix64, the same stream the binary gives plugins.
function nextRandom(state) {
  state.value = BigInt.asUintN(64, state.value + 0x9e3779b97f4a7c15n);
  let z = state.value;
  z = BigInt.asUintN(64, (z ^ (z >> 30n)) * 0xbf58476d1ce4e5b9n);
  z = BigInt.asUintN(64, (z ^ (z >> 27n)) * 0x94d049bb133111ebn);
  return z ^ (z >> 31n);
}

function toBase64(bytes) {
  let binary = "";
  for (let i = 0; i < bytes.length; i += 0x8000) binary += String.fromCharCode(...bytes.subarray(i, i + 0x8000));
  return btoa(binary);
}

/**
 * Loads the core module with a plugin host.
 * @returns {Promise<{ call: (request: object) => any, loadPlugin: (files: Map<string, Uint8Array>) => any }>}
 */
export async function loadCore(source) {
  const plugins = new Map();
  let core = null;

  // Runs one plugin request in a fresh instance, as the binary does, so the
  // same request always gives the same answer.
  function runPlugin(plugin, request, seed) {
    let current = null;
    const rng = { value: seed };
    const read = (ptr, len) => new Uint8Array(current.exports.memory.buffer, ptr, len).slice();
    const give = (bytes) => {
      const ptr = current.exports.dedalo_alloc(bytes.length);
      new Uint8Array(current.exports.memory.buffer, ptr, bytes.length).set(bytes);
      return BigInt.asIntN(64, (BigInt(ptr >>> 0) << 32n) | BigInt(bytes.length));
    };
    const imports = {
      dedalo: {
        log: (level, ptr, len) => console.debug(`[${plugin.id}] ${decoder.decode(read(ptr, len))}`),
        rand_u64: () => BigInt.asIntN(64, nextRandom(rng)),
        asset: (ptr, len) => {
          const file = plugin.files.get(decoder.decode(read(ptr, len)));
          return file ? give(file) : 0n;
        },
        measure_text: (ptr, len) => {
          const bytes = read(ptr, len);
          const e = core.exports;
          const inPtr = e.dedalo_alloc(bytes.length);
          new Uint8Array(e.memory.buffer, inPtr, bytes.length).set(bytes);
          const packed = BigInt.asUintN(64, BigInt(e.dedalo_measure(inPtr, bytes.length)));
          e.dedalo_free(inPtr, bytes.length);
          const out = new Uint8Array(e.memory.buffer, Number(packed >> 32n), Number(packed & 0xffffffffn)).slice();
          e.dedalo_free(Number(packed >> 32n), Number(packed & 0xffffffffn));
          return give(out);
        },
      },
    };
    current = new WebAssembly.Instance(plugin.module, imports);
    return packedCall(current.exports, request);
  }

  const host = {
    dedalo_host: {
      plugin_call(idPtr, idLen, reqPtr, reqLen, seed) {
        const memory = core.exports.memory.buffer;
        const id = decoder.decode(new Uint8Array(memory, idPtr, idLen));
        const request = new Uint8Array(memory, reqPtr, reqLen).slice();
        const plugin = plugins.get(id);
        if (!plugin) return 0n;
        let answer;
        try {
          answer = runPlugin(plugin, request, BigInt.asUintN(64, BigInt(seed)));
        } catch (error) {
          answer = encoder.encode(JSON.stringify({ error: { message: `the plugin trapped: ${error.message}` } }));
        }
        const ptr = core.exports.dedalo_alloc(answer.length);
        new Uint8Array(core.exports.memory.buffer, ptr, answer.length).set(answer);
        return BigInt.asIntN(64, (BigInt(ptr >>> 0) << 32n) | BigInt(answer.length));
      },
    },
  };
  ({ instance: core } = await WebAssembly.instantiate(await bytesOf(source), host));
  const call = (request) => JSON.parse(decoder.decode(packedCall(core.exports, request)));

  /** Loads a plugin package given as a Map of path to bytes. */
  async function loadPlugin(files) {
    const encoded = {};
    for (const [path, bytes] of files) encoded[path] = toBase64(bytes);
    const result = call({ cmd: "load_plugin", files: encoded });
    if (!result.ok) return result;
    const manifest = decoder.decode(files.get("dedalo-plugin.toml"));
    const wasmPath = manifest.match(/\[wasm\][^[]*?module\s*=\s*"([^"]+)"/)?.[1];
    if (wasmPath) {
      // Same rule as the binary: only `dedalo.*` imports.
      const module = await WebAssembly.compile(files.get(wasmPath));
      const bad = WebAssembly.Module.imports(module).find((i) => i.module !== "dedalo");
      if (bad) return { ok: false, error: `the module imports ${bad.module}.${bad.name}` };
      plugins.set(result.plugin.id, { id: result.plugin.id, module, files });
    }
    return result;
  }

  return { call, loadPlugin };
}

/**
 * The files of a plugin package, fetched from a folder URL: those `listed`
 * (the plugin index lists every file of a package), else those the
 * manifest names.
 */
export async function fetchPackage(baseUrl, readBytes = null, listed = null) {
  const get = readBytes || (async (path) => new Uint8Array(await (await fetch(new URL(path, baseUrl))).arrayBuffer()));
  const files = new Map();
  if (listed) {
    for (const path of listed) files.set(path, await get(path));
    return files;
  }
  const manifestBytes = await get("dedalo-plugin.toml");
  files.set("dedalo-plugin.toml", manifestBytes);
  const manifest = decoder.decode(manifestBytes);
  for (const [, path] of manifest.matchAll(/^\s*(?:file|module)\s*=\s*"([^"]+)"/gm)) {
    files.set(path, await get(path));
  }
  return files;
}
