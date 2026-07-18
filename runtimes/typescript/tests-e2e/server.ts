// Tiny static file server for the built e2e fixture. No dev server, no child process — just
// enough to serve the vite bundle with correct MIME types. `.wasm` must be `application/wasm`
// or the browser's WebAssembly.instantiateStreaming rejects (the sahou core web target uses it).
import { createReadStream, existsSync, statSync } from "node:fs";
import { createServer, type Server } from "node:http";
import { extname, join, normalize, resolve } from "node:path";

const MIME: Record<string, string> = {
  ".html": "text/html; charset=utf-8",
  ".js": "text/javascript; charset=utf-8",
  ".mjs": "text/javascript; charset=utf-8",
  ".wasm": "application/wasm",
  ".json": "application/json; charset=utf-8",
  ".map": "application/json; charset=utf-8",
};

export interface StaticServer {
  port: number;
  close(): Promise<void>;
}

/** Serve `root` on `port` (loopback). Strips the query string and pins every path inside `root`. */
export async function serveDir(root: string, port: number): Promise<StaticServer> {
  const base = resolve(root);
  const server: Server = createServer((req, res) => {
    let rel = decodeURIComponent((req.url ?? "/").split("?")[0]); // drop the query string
    if (rel.endsWith("/")) rel += "index.html";
    const file = normalize(join(base, rel));
    if (!file.startsWith(base) || !existsSync(file) || !statSync(file).isFile()) {
      res.statusCode = 404;
      res.end("not found");
      return;
    }
    res.setHeader("content-type", MIME[extname(file)] ?? "application/octet-stream");
    createReadStream(file).pipe(res);
  });
  await new Promise<void>((res) => server.listen(port, "127.0.0.1", res));
  return {
    port,
    close: () => new Promise<void>((res) => server.close(() => res())),
  };
}
