// Runs before the editor's frame loads: a clean slate, in English.
for (const key of ["dedalo.draft", "dedalo.plugins", "dedalo.pluginUrls"]) localStorage.removeItem(key);
localStorage.setItem("dedalo.lang", "en");
