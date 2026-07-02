#!/usr/bin/env node

const { mkdirSync, chmodSync, createWriteStream } = require("fs");
const { join } = require("path");
const { platform, arch } = require("os");
const { get } = require("https");
const { createGunzip } = require("zlib");
const { pipeline } = require("stream");

const VERSION = "v2.3.0";
const EXE = platform === "win32" ? ".exe" : "";
const BIN_DIR = join(__dirname, "bin");

const targets = {
  "darwin-x64": "x86_64-apple-darwin",
  "darwin-arm64": "aarch64-apple-darwin",
  "linux-x64": "x86_64-unknown-linux-gnu",
  "linux-arm64": "aarch64-unknown-linux-gnu",
  "win32-x64": "x86_64-pc-windows-gnu",
};

const key = `${platform}-${arch}`;
const target = targets[key];

if (!target) {
  console.error(`xei: unsupported platform ${platform}-${arch}`);
  process.exit(1);
}

mkdirSync(BIN_DIR, { recursive: true });

let xeiDone = false;
let suiseiDone = false;
let xeiFailed = false;

function maybeExit() {
  if (xeiDone && suiseiDone) {
    if (xeiFailed) process.exit(1);
  }
}

download("xei", target, () => { xeiDone = true; maybeExit(); });

if (platform === "darwin") {
  download("suisei", target, () => { suiseiDone = true; maybeExit(); });
} else {
  suiseiDone = true;
}

function download(name, target, done) {
  const binPath = join(BIN_DIR, `${name}${EXE}`);
  try { require("fs").unlinkSync(binPath); } catch (_) {}

  const url = `https://github.com/stremtec/xei/releases/download/${VERSION}/${name}-${target}${EXE}.gz`;

  get(url, (res) => {
    if (res.statusCode === 302 || res.statusCode === 301) {
      get(res.headers.location, onResponse).on("error", (err) => onError(err));
      return;
    }
    onResponse(res);
  }).on("error", onError);

  function onResponse(res) {
    if (res.statusCode !== 200) {
      if (name === "suisei") {
        console.log(`suisei: desktop editor not available for ${platform}, skipping`);
        done(); return;
      }
      console.error(`${name}: HTTP ${res.statusCode}`);
      xeiFailed = true; done(); return;
    }
    const file = createWriteStream(binPath);
    pipeline(res, createGunzip(), file, (err) => {
      if (err) {
        if (name === "suisei") { console.log(`suisei: skipped (${err.message})`); done(); return; }
        console.error(`${name}: ${err.message}`);
        xeiFailed = true; done(); return;
      }
      try { chmodSync(binPath, 0o755); } catch (_) {}
      done();
    });
  }

  function onError(err) {
    if (name === "suisei") { console.log(`suisei: skipped`); done(); return; }
    console.error(`${name}: download failed: ${err.message}`);
    xeiFailed = true; done();
  }
}
