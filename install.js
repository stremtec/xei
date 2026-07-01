#!/usr/bin/env node

const { existsSync, mkdirSync, chmodSync, createWriteStream } = require("fs");
const { join } = require("path");
const { platform, arch } = require("os");
const { get } = require("https");
const { createGunzip } = require("zlib");
const { pipeline } = require("stream");

const VERSION = "v0.1.2";
const BIN_DIR = join(__dirname, "bin");
const BIN_PATH = join(BIN_DIR, "xei");

const targets = {
  "darwin-x64": "x86_64-apple-darwin",
  "darwin-arm64": "aarch64-apple-darwin",
  "linux-x64": "x86_64-unknown-linux-gnu",
  "linux-arm64": "aarch64-unknown-linux-gnu",
};

const key = `${platform}-${arch}`;
const target = targets[key];

if (!target) {
  console.error(`xei: unsupported platform ${platform}-${arch}`);
  process.exit(1);
}

if (existsSync(BIN_PATH)) {
  process.exit(0);
}

const url = `https://github.com/stremtec/xei/releases/download/${VERSION}/xei-${target}.gz`;

mkdirSync(BIN_DIR, { recursive: true });

get(url, (res) => {
  if (res.statusCode === 302 || res.statusCode === 301) {
    get(res.headers.location, onResponse).on("error", onError);
    return;
  }
  onResponse(res);
}).on("error", onError);

function onResponse(res) {
  if (res.statusCode !== 200) {
    console.error(`xei: HTTP ${res.statusCode} — binary not found for ${target}`);
    console.error("xei: install via cargo instead:  cargo install xei");
    process.exit(1);
  }
  const file = createWriteStream(BIN_PATH);
  pipeline(res, createGunzip(), file, (err) => {
    if (err) { onError(err); return; }
    chmodSync(BIN_PATH, 0o755);
  });
}

function onError(err) {
  console.error(`xei: download failed: ${err.message}`);
  process.exit(1);
}
