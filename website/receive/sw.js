// Service Worker của trang nhận: biến dữ liệu đã giải mã (trang đẩy sang từng khối) thành một lượt
// tải xuống bình thường của trình duyệt, để file lớn được ghi thẳng xuống đĩa thay vì giữ trong RAM.
// Dùng khi trình duyệt không có showSaveFilePicker (vd Firefox).
const pending = new Map();

self.addEventListener("install", () => self.skipWaiting());
self.addEventListener("activate", (e) => e.waitUntil(self.clients.claim()));

self.addEventListener("message", (e) => {
  const d = e.data || {};
  if (d.type === "bd-download" && e.ports[0]) {
    pending.set(d.id, { port: e.ports[0], name: String(d.name || "betterdrop"), size: Number(d.size) || 0 });
    // Bỏ lượt tải không ai mở sau 1 phút.
    setTimeout(() => pending.delete(d.id), 60_000);
  }
  // "bd-ping": trang nhắc đều để trình duyệt không tắt SW giữa chừng.
});

self.addEventListener("fetch", (e) => {
  const m = /\/dl\/([a-z0-9]+)\//.exec(new URL(e.request.url).pathname);
  const job = m && pending.get(m[1]);
  if (!job) return;
  pending.delete(m[1]);
  const { port } = job;
  const stream = new ReadableStream({
    start(ctrl) {
      port.onmessage = ({ data }) => {
        if (data.type === "chunk") ctrl.enqueue(new Uint8Array(data.chunk));
        else if (data.type === "end") ctrl.close();
        else if (data.type === "abort") ctrl.error(new Error("aborted"));
      };
    },
    pull() {
      port.postMessage({ type: "pull" });
    },
    cancel() {
      port.postMessage({ type: "cancel" });
    },
  });
  const headers = {
    "Content-Type": "application/octet-stream",
    "Content-Disposition": `attachment; filename*=UTF-8''${encodeURIComponent(job.name)}`,
    "X-Content-Type-Options": "nosniff",
  };
  if (job.size) headers["Content-Length"] = String(job.size);
  e.respondWith(new Response(stream, { headers }));
});
