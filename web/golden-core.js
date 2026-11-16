// Cross-target golden check (spec 15.1): the WebAssembly build must produce,
// for every corpus file, exactly the bytes the native build recorded.
//
// `call` is the function returned by `loadDedalo`. `readText` reads a file
// of the corpus by relative path, with line endings as stored in the
// repository. The same function runs in Node (CI) and in a browser.

export async function runGolden(call, readText) {
  const index = JSON.parse(await readText("index.json"));
  const failures = [];
  let files = 0;
  for (const [name, expectedFiles] of Object.entries(index)) {
    const source = await readText(`${name}/input.dedalo`);
    const actual = call({ cmd: "golden", source }).files;
    const actualNames = Object.keys(actual).sort();
    if (JSON.stringify(actualNames) !== JSON.stringify([...expectedFiles].sort())) {
      failures.push(`${name}: produced ${actualNames.join(", ")}; expected ${expectedFiles.join(", ")}`);
      continue;
    }
    for (const file of expectedFiles) {
      const expected = await readText(`${name}/${file}`);
      files += 1;
      if (expected !== actual[file]) {
        failures.push(`${name}/${file}: differs from the native output`);
      }
    }
  }
  return { cases: Object.keys(index).length, files, failures };
}
