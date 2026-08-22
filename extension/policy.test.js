const { test } = require("node:test");
const assert = require("node:assert/strict");
const {
  shouldTakeover,
  isBlobLike,
  decodeDataUrl,
  blobId,
  itemKey,
  hostDenied,
  inlineTooLarge,
  MAX_INLINE_BYTES,
} = require("./policy.js");

test("大文件接管", () => {
  assert.equal(shouldTakeover({
    url: "https://cdn.example/a.zip", fileSize: 2e6, mime: "application/zip",
  }), true);
  assert.equal(shouldTakeover({ url: "blob:https://gemini.google.com/abc" }), true);
  assert.equal(shouldTakeover({ url: "data:image/png;base64,AAAA" }), true);
  assert.equal(shouldTakeover({ url: "blob:null/f1f1a92e-7623-4c61-938b-a5667fe04ebd" }), true);
});

test("体积未知不套阈值, 已知小于 1MB 不接管", () => {
  assert.equal(shouldTakeover({
    url: "https://cdn.example/a.bin", fileSize: 0, mime: "application/octet-stream",
  }), true);
  assert.equal(shouldTakeover({
    url: "https://cdn.example/a.bin", fileSize: 12, mime: "application/octet-stream",
  }), false);
});

test("fileSize 为 -1/0 时用 totalBytes 套阈值", () => {
  assert.equal(shouldTakeover({
    url: "https://cdn.example/a.bin", fileSize: -1, totalBytes: 12, mime: "application/octet-stream",
  }), false);
  assert.equal(shouldTakeover({
    url: "https://cdn.example/a.bin", fileSize: 0, totalBytes: 500000, mime: "application/octet-stream",
  }), false);
  assert.equal(shouldTakeover({
    url: "https://cdn.example/a.bin", fileSize: -1, totalBytes: 2e6, mime: "application/octet-stream",
  }), true);
});

test("text/html 不接管", () => {
  assert.equal(shouldTakeover({
    url: "https://example.com/a.html", fileSize: 2e6, mime: "text/html",
  }), false);
  assert.equal(shouldTakeover({
    url: "https://example.com/a.html", fileSize: 2e6, mime: "text/html; charset=utf-8",
  }), false);
});

test("域名黑名单匹配自身和子域", () => {
  const rules = { minBytes: 1, denyHosts: ["evil.com", "*.ads.net"] };
  assert.equal(shouldTakeover({
    url: "https://evil.com/a.bin", fileSize: 2e6,
  }, rules), false);
  assert.equal(shouldTakeover({
    url: "https://cdn.evil.com/a.bin", fileSize: 2e6,
  }, rules), false);
  assert.equal(shouldTakeover({
    url: "https://ads.net/a.bin", fileSize: 2e6,
  }, rules), false);
  assert.equal(shouldTakeover({
    url: "https://cdn.example/a.bin", fileSize: 2e6,
  }, rules), true);
});

test("minBytes=0 关闭体积过滤", () => {
  assert.equal(shouldTakeover({
    url: "https://cdn.example/tiny.bin", fileSize: 12, mime: "application/octet-stream",
  }, { minBytes: 0 }), true);
});

test("浏览器内部协议不接管", () => {
  assert.equal(shouldTakeover({ url: "chrome://downloads" }), false);
  assert.equal(shouldTakeover({ url: "chrome-extension://abc/a.bin" }), false);
  assert.equal(shouldTakeover({ url: "about:blank" }), false);
  assert.equal(shouldTakeover({ url: "" }), false);
  assert.equal(shouldTakeover({}), false);
});

test("blob:null 与页面 blob URL 同一 uuid", () => {
  const id = "f1f1a92e-7623-4c61-938b-a5667fe04ebd";
  assert.equal(isBlobLike("blob:null/" + id), true);
  assert.equal(blobId("blob:null/" + id), id);
  assert.equal(blobId("blob:https://gemini.google.com/" + id), id);
  assert.equal(itemKey("blob:null/" + id), itemKey("blob:https://gemini.google.com/" + id));
});

test("data URL 解码", () => {
  const d = decodeDataUrl("data:text/plain;base64,aGk=");
  assert.equal(d.mime, "text/plain");
  assert.equal(d.b64, "aGk=");
});

test("hostDenied", () => {
  assert.equal(hostDenied("a.evil.com", ["evil.com"]), true);
  assert.equal(hostDenied("evil.com", ["evil.com"]), true);
  assert.equal(hostDenied("not-evil.com", ["evil.com"]), false);
});

test("inline 上限 24MB, 对齐引擎导入", () => {
  assert.equal(MAX_INLINE_BYTES, 24 * 1024 * 1024);
  assert.equal(inlineTooLarge(MAX_INLINE_BYTES), false);
  assert.equal(inlineTooLarge(MAX_INLINE_BYTES + 1), true);
});
