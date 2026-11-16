// Loads the Dedalo core (WebAssembly) and returns a synchronous `call`.
//
// The module exposes three raw exports: `dedalo_alloc`, `dedalo_free` and
// `dedalo_call`. Requests and responses are JSON. This file is the whole
// binding layer of milestone M0; it runs unchanged in browsers and in Node.

/**
 * @param {string | URL | ArrayBuffer | Uint8Array} source A URL to fetch, or the module bytes.
 * @returns {Promise<(request: object) => any>}
 */
export async function loadDedalo(source) {
  let bytes = source;
  if (typeof source === "string" || source instanceof URL) {
    const response = await fetch(source);
    if (!response.ok) {
      throw new Error(`cannot load ${source}: HTTP ${response.status}`);
    }
    bytes = await response.arrayBuffer();
  }
  // The core imports nothing: it has no access to files, network or clock.
  const { instance } = await WebAssembly.instantiate(bytes, {});
  const { memory, dedalo_alloc, dedalo_free, dedalo_call } = instance.exports;
  const encoder = new TextEncoder();
  const decoder = new TextDecoder();

  return function call(request) {
    const input = encoder.encode(JSON.stringify(request));
    const inputPtr = dedalo_alloc(input.length);
    new Uint8Array(memory.buffer, inputPtr, input.length).set(input);
    // The result packs a pointer in the high 32 bits and a length in the low.
    const packed = BigInt.asUintN(64, dedalo_call(inputPtr, input.length));
    dedalo_free(inputPtr, input.length);
    const outputPtr = Number(packed >> 32n);
    const outputLen = Number(packed & 0xffffffffn);
    const text = decoder.decode(new Uint8Array(memory.buffer, outputPtr, outputLen));
    dedalo_free(outputPtr, outputLen);
    return JSON.parse(text);
  };
}
