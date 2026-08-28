#!/usr/bin/env deno
// Vendor the pinned llama.cpp release into ~/.flint/bin/ (dev path). The release is
// pinned by tag + sha256 (3b-M1 decision: official release, not build-from-source).
// The bundled externalBin recipe (3a-M2) copies from here at build time.
//
// Usage: deno task fetch-llama-server

const PIN = {
  tag: "b10667",
  url: "https://github.com/ggml-org/llama.cpp/releases/download/b10667/llama-b10667-bin-macos-arm64.tar.gz",
  sha256: "101bf7015b4d07dae2df8a46450c78e4d2a740f3fcf6fbb2b9c099e78e20cdac",
};

const home = Deno.env.get("HOME") ?? ".";
const destDir = `${home}/.flint/bin`;
const tmp = `${destDir}/llama.tar.gz`;
await Deno.mkdir(destDir, { recursive: true });

console.log(`fetching llama.cpp ${PIN.tag} ...`);
const out = await Deno.open(tmp, { create: true, write: true, truncate: true });
const resp = await fetch(PIN.url);
if (!resp.ok || !resp.body) throw new Error(`download failed: ${resp.status}`);
await resp.body.pipeTo(out.writable);

const digest = new Uint8Array(await crypto.subtle.digest("SHA-256", await Deno.readFile(tmp)));
const hex = [...digest].map((b) => b.toString(16).padStart(2, "0")).join("");
if (hex !== PIN.sha256) {
  await Deno.remove(tmp);
  throw new Error(`sha256 mismatch: got ${hex}, expected ${PIN.sha256}`);
}
console.log("sha256 verified");

// The release tarball extracts to <tag>/ with llama-server + all its dylibs at top
// level. Copy the binary and every dylib (llama-server's @rpath resolves against its
// own directory), then drop the extraction dir.
await new Deno.Command("tar", { args: ["xzf", tmp, "-C", destDir] }).output();
await Deno.remove(tmp);
const src = `${destDir}/llama-b${PIN.tag.replace(/^b/, "")}`;
const entries = [...Deno.readDirSync(src)];
for (const e of entries) {
  if (e.name === "llama-server" || e.name.endsWith(".dylib")) {
    await Deno.copyFile(`${src}/${e.name}`, `${destDir}/${e.name}`);
  }
}
await new Deno.Command("rm", { args: ["-rf", src] }).output();
await Deno.chmod(`${destDir}/llama-server`, 0o755);
console.log(`installed llama-server to ${destDir}/llama-server`);
