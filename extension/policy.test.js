const { test } = require("node:test");
const assert = require("node:assert/strict");
const {
  shouldTakeover,
  isBlobLike,
  decodeDataUrl,
  blobId,
  itemKey,
} = require("./policy.js");

test("http/https/blob/data 都接管, 不再看体积和 mime", () => {
  assert.equal(shouldTakeover({ url: "https://lh3.googleusercontent.com/x", fileSize: 12, mime: "image/png" }), true);
  assert.equal(shouldTakeover({ url: "https://example.com/a.html", fileSize: 100, mime: "text/html" }), true);
  assert.equal(shouldTakeover({ url: "http://127.0.0.1/logo.png", fileSize: 512 }), true);
  assert.equal(shouldTakeover({ url: "blob:https://gemini.google.com/abc" }), true);
  assert.equal(shouldTakeover({ url: "data:image/png;base64,AAAA" }), true);
  assert.equal(shouldTakeover({ url: "blob:null/f1f1a92e-7623-4c61-938b-a5667fe04ebd" }), true);
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
