import http from "node:http";
import { readFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";

const root = path.dirname(fileURLToPath(import.meta.url));
const port = Number(process.env.PORT ?? 4177);

const server = http.createServer(async (request, response) => {
  const url = new URL(request.url ?? "/", `http://${request.headers.host}`);
  if (url.pathname.startsWith("/files/")) {
    return serveFixture(url.pathname, response);
  }

  const requested = url.pathname === "/" ? "/index.html" : url.pathname;
  const file = path.join(root, path.basename(requested));
  try {
    const body = await readFile(file);
    response.writeHead(200, {
      "content-type": contentType(file),
      "cache-control": "no-store",
    });
    response.end(body);
  } catch {
    response.writeHead(404, { "content-type": "text/plain; charset=utf-8" });
    response.end("Not found");
  }
});

server.listen(port, "127.0.0.1", () => {
  console.log(`Ravyn extension fixtures: http://127.0.0.1:${port}`);
});

function serveFixture(pathname, response) {
  if (pathname.endsWith("playlist.m3u8")) {
    response.writeHead(200, {
      "content-type": "application/vnd.apple.mpegurl",
    });
    return response.end(
      "#EXTM3U\n#EXT-X-TARGETDURATION:4\n#EXTINF:4,\nsegment-1.ts\n#EXT-X-ENDLIST\n",
    );
  }
  if (pathname.endsWith("manifest.mpd")) {
    response.writeHead(200, { "content-type": "application/dash+xml" });
    return response.end('<?xml version="1.0"?><MPD type="static"></MPD>');
  }
  if (pathname.endsWith("image.svg")) {
    response.writeHead(200, { "content-type": "image/svg+xml" });
    return response.end(
      '<svg xmlns="http://www.w3.org/2000/svg" width="640" height="360"><rect width="100%" height="100%" fill="#566074"/><text x="40" y="190" fill="white" font-size="42">Ravyn fixture</text></svg>',
    );
  }
  const attachment = pathname.includes("attachment");
  response.writeHead(200, {
    "content-type": pathname.endsWith(".mp4")
      ? "video/mp4"
      : "application/octet-stream",
    ...(attachment
      ? { "content-disposition": 'attachment; filename="ravyn-fixture.bin"' }
      : {}),
  });
  response.end(Buffer.alloc(1024, 0x52));
}

function contentType(file) {
  return file.endsWith(".html")
    ? "text/html; charset=utf-8"
    : "text/plain; charset=utf-8";
}
