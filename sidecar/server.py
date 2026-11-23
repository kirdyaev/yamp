from __future__ import annotations

import json
import logging
import sys
import threading
import time
import urllib.parse
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from typing import Any, Optional

import requests
from yandex_music import Client

logging.getLogger("yandex_music").setLevel(logging.CRITICAL)
logging.getLogger("urllib3").setLevel(logging.CRITICAL)


class SidecarState:
    def __init__(self, secret: str, stream_nonce: str, token: Optional[str]) -> None:
        self.lock = threading.Lock()
        self.secret = secret
        self.stream_nonce = stream_nonce
        self._token: Optional[str] = token.strip() if token and token.strip() else None
        self._client: Optional[Client] = None
        self._download_cache: dict[str, tuple[float, str, str]] = {}

    def has_token(self) -> bool:
        with self.lock:
            return bool(self._token)

    def set_token(self, token: Optional[str]) -> None:
        cleaned = token.strip() if token and token.strip() else None
        with self.lock:
            self._token = cleaned
            self._client = None
            self._download_cache.clear()

    def get_client(self) -> Client:
        with self.lock:
            if not self._token:
                raise PermissionError("NO_TOKEN")
            if self._client is None:
                self._client = Client(self._token).init()
            return self._client

    def resolve_stream_url(self, track_id: str) -> tuple[str, str]:
        now = time.monotonic()
        with self.lock:
            cached = self._download_cache.get(track_id)
            if cached and cached[0] > now:
                return cached[1], cached[2]

        client = self.get_client()
        infos = client.tracks_download_info(track_id, get_direct_links=True)
        if not infos:
            raise RuntimeError("No download info available for track")

        mp3_infos = [i for i in infos if getattr(i, "codec", None) == "mp3" and getattr(i, "direct_link", None)]
        candidates = mp3_infos if mp3_infos else [i for i in infos if getattr(i, "direct_link", None)]
        if not candidates:
            raise RuntimeError("No direct stream URL available for track")

        best = max(candidates, key=lambda i: getattr(i, "bitrate_in_kbps", 0) or 0)
        direct_url = best.direct_link
        codec = getattr(best, "codec", None) or "mp3"
        content_type = "audio/mpeg" if codec == "mp3" else "audio/mp4"

        with self.lock:
            self._download_cache[track_id] = (time.monotonic() + 90.0, direct_url, content_type)
        return direct_url, content_type


STATE: SidecarState


def serialize_track(track: Any, base_stream_url: str) -> dict[str, Any]:
    raw_id = getattr(track, "track_id", None) or getattr(track, "id", "")
    track_id = str(raw_id)
    title = getattr(track, "title", None) or "Untitled"
    artists = getattr(track, "artists", None) or []
    artist_names = [a.name for a in artists if getattr(a, "name", None)]
    artist = ", ".join(artist_names) if artist_names else "Unknown Artist"
    albums = getattr(track, "albums", None) or []
    album = albums[0].title if albums and getattr(albums[0], "title", None) else ""
    duration_ms = getattr(track, "duration_ms", None) or 0
    return {
        "id": track_id,
        "title": title,
        "artist": artist,
        "album": album,
        "duration": round(duration_ms / 1000.0, 2),
        "streamUrl": f"{base_stream_url}/{urllib.parse.quote(track_id, safe='')}",
    }


def _chunked(items: list[str], size: int = 100) -> list[list[str]]:
    return [items[i : i + size] for i in range(0, len(items), size)]


class YamampHandler(BaseHTTPRequestHandler):
    protocol_version = "HTTP/1.1"

    def log_message(self, format: str, *args: Any) -> None:  # noqa: A002
        return

    def _base_stream_url(self) -> str:
        port = self.server.server_address[1]
        return f"http://127.0.0.1:{port}/s/{STATE.stream_nonce}/stream"

    def _check_secret(self) -> bool:
        got = self.headers.get("X-Yamamp-Secret", "")
        return got == STATE.secret

    def _send_json(self, status: int, payload: dict[str, Any]) -> None:
        body = json.dumps(payload, ensure_ascii=False).encode("utf-8")
        self.send_response(status)
        self.send_header("Content-Type", "application/json; charset=utf-8")
        self.send_header("Content-Length", str(len(body)))
        self.send_header("Cache-Control", "no-store")
        self.end_headers()
        if self.command != "HEAD":
            self.wfile.write(body)

    def _send_cors_headers(self) -> None:
        self.send_header("Access-Control-Allow-Origin", "*")
        self.send_header("Access-Control-Allow-Methods", "GET, HEAD, OPTIONS")
        self.send_header("Access-Control-Allow-Headers", "Range")
        self.send_header("Access-Control-Expose-Headers", "Content-Length, Content-Range, Accept-Ranges")

    def do_OPTIONS(self) -> None:
        parsed = urllib.parse.urlparse(self.path)
        prefix = f"/s/{STATE.stream_nonce}/stream/"
        if not parsed.path.startswith(prefix):
            self.send_response(404)
            self.send_header("Content-Length", "0")
            self.end_headers()
            return
        self.send_response(204)
        self._send_cors_headers()
        self.send_header("Content-Length", "0")
        self.end_headers()

    def do_HEAD(self) -> None:
        self._handle_get_or_head(head_only=True)

    def do_GET(self) -> None:
        self._handle_get_or_head(head_only=False)

    def do_POST(self) -> None:
        if not self._check_secret():
            self._send_json(403, {"error": "FORBIDDEN"})
            return

        parsed = urllib.parse.urlparse(self.path)
        if parsed.path != "/internal/token":
            self._send_json(404, {"error": "NOT_FOUND"})
            return

        try:
            length = int(self.headers.get("Content-Length", "0"))
            raw = self.rfile.read(length)
            data = json.loads(raw.decode("utf-8")) if raw else {}
            token = data.get("token")
            if token is not None and not isinstance(token, str):
                self._send_json(400, {"error": "INVALID_TOKEN_TYPE"})
                return
            STATE.set_token(token)
            self._send_json(200, {"ok": True, "hasToken": STATE.has_token()})
        except Exception as exc:
            self._send_json(500, {"error": f"Failed to update token: {type(exc).__name__}"})

    def _handle_get_or_head(self, head_only: bool) -> None:
        parsed = urllib.parse.urlparse(self.path)
        stream_prefix = f"/s/{STATE.stream_nonce}/stream/"
        if parsed.path.startswith(stream_prefix):
            raw_track_id = parsed.path[len(stream_prefix) :]
            track_id = urllib.parse.unquote(raw_track_id)
            self._handle_stream(track_id, head_only=head_only)
            return

        if parsed.path.startswith("/s/"):
            self._send_json(403, {"error": "FORBIDDEN"})
            return

        if not self._check_secret():
            self._send_json(403, {"error": "FORBIDDEN"})
            return

        if parsed.path == "/health":
            self._send_json(200, {"ok": True, "hasToken": STATE.has_token()})
            return

        try:
            base_stream = self._base_stream_url()
            qs = urllib.parse.parse_qs(parsed.query)

            if parsed.path == "/api/search":
                q = (qs.get("q") or [""])[0].strip()
                page = int((qs.get("page") or ["0"])[0])
                if not q:
                    self._send_json(200, {"tracks": [], "total": 0})
                    return
                client = STATE.get_client()
                res = client.search(q, type_="track", page=page)
                tracks_block = getattr(res, "tracks", None)
                results = getattr(tracks_block, "results", None) or []
                total = getattr(tracks_block, "total", len(results)) or len(results)
                self._send_json(
                    200,
                    {
                        "tracks": [serialize_track(t, base_stream) for t in results],
                        "total": total,
                    },
                )
                return

            if parsed.path == "/api/playlists":
                client = STATE.get_client()
                pls = client.users_playlists_list() or []
                items = []
                for p in pls:
                    owner = getattr(p, "owner", None)
                    owner_id = str(owner.uid) if owner and getattr(owner, "uid", None) is not None else ""
                    items.append(
                        {
                            "kind": getattr(p, "kind", 0),
                            "ownerId": owner_id,
                            "title": getattr(p, "title", None) or "Untitled",
                            "trackCount": getattr(p, "track_count", None) or 0,
                        }
                    )
                self._send_json(200, {"playlists": items})
                return

            if parsed.path.startswith("/api/playlists/"):
                parts = parsed.path[len("/api/playlists/") :].split("/")
                if len(parts) != 2:
                    self._send_json(404, {"error": "NOT_FOUND"})
                    return
                owner_id = urllib.parse.unquote(parts[0]).strip()
                kind = int(parts[1])
                client = STATE.get_client()
                pl = client.users_playlists(kind=kind, user_id=owner_id or None)
                short_tracks = (getattr(pl, "tracks", None) or [])[:200]
                hydrated = self._hydrate_short_tracks(client, short_tracks)
                self._send_json(
                    200,
                    {
                        "title": getattr(pl, "title", None) or "Playlist",
                        "tracks": [serialize_track(t, base_stream) for t in hydrated],
                    },
                )
                return

            if parsed.path == "/api/likes":
                client = STATE.get_client()
                likes = client.users_likes_tracks()
                all_short = getattr(likes, "tracks", None) or []
                hydrated = self._hydrate_short_tracks(client, all_short[:200])
                self._send_json(
                    200,
                    {
                        "tracks": [serialize_track(t, base_stream) for t in hydrated],
                        "total": len(all_short),
                    },
                )
                return

            if parsed.path == "/api/wave":
                queue = (qs.get("queue") or [""])[0].strip() or None
                client = STATE.get_client()
                station = client.rotor_station_tracks("user:onyourwave", queue=queue)
                seq = getattr(station, "sequence", None) or []
                tracks = [item.track for item in seq if getattr(item, "track", None) is not None]
                self._send_json(
                    200,
                    {
                        "tracks": [serialize_track(t, base_stream) for t in tracks],
                        "batchId": getattr(station, "batch_id", None),
                    },
                )
                return

            self._send_json(404, {"error": "NOT_FOUND"})
        except PermissionError:
            self._send_json(401, {"error": "NO_TOKEN"})
        except Exception as exc:
            self._send_json(500, {"error": f"{type(exc).__name__}: {exc}"})

    def _hydrate_short_tracks(self, client: Client, short_tracks: list[Any]) -> list[Any]:
        if not short_tracks:
            return []
        already = [st.track for st in short_tracks if getattr(st, "track", None) is not None]
        if len(already) == len(short_tracks):
            return already
        ids = [str(st.track_id) for st in short_tracks if getattr(st, "track_id", None)]
        out: list[Any] = []
        for batch in _chunked(ids, 100):
            out.extend(client.tracks(batch) or [])
        return out

    def _handle_stream(self, track_id: str, head_only: bool) -> None:
        if not track_id:
            self.send_response(400)
            self._send_cors_headers()
            self.send_header("Content-Length", "0")
            self.end_headers()
            return

        try:
            direct_url, default_ct = STATE.resolve_stream_url(track_id)
        except PermissionError:
            self.send_response(401)
            self._send_cors_headers()
            self.send_header("Content-Length", "0")
            self.end_headers()
            return
        except Exception:
            self.send_response(502)
            self._send_cors_headers()
            self.send_header("Content-Length", "0")
            self.end_headers()
            return

        upstream_headers: dict[str, str] = {}
        range_hdr = self.headers.get("Range")
        if range_hdr:
            upstream_headers["Range"] = range_hdr

        try:
            method = "HEAD" if head_only else "GET"
            with requests.request(
                method,
                direct_url,
                headers=upstream_headers,
                stream=not head_only,
                timeout=20,
            ) as resp:
                self.send_response(resp.status_code)
                self._send_cors_headers()
                self.send_header("Content-Type", resp.headers.get("Content-Type") or default_ct)
                self.send_header("Accept-Ranges", resp.headers.get("Accept-Ranges") or "bytes")
                if "Content-Length" in resp.headers:
                    self.send_header("Content-Length", resp.headers["Content-Length"])
                if "Content-Range" in resp.headers:
                    self.send_header("Content-Range", resp.headers["Content-Range"])
                self.send_header("Cache-Control", "no-store")
                self.end_headers()

                if not head_only:
                    for chunk in resp.iter_content(chunk_size=65536):
                        if chunk:
                            self.wfile.write(chunk)
        except (BrokenPipeError, ConnectionResetError):
            return
        except Exception:
            return


def main() -> None:
    global STATE
    line = sys.stdin.readline()
    if not line:
        raise SystemExit(1)
    boot = json.loads(line)
    secret = boot["secret"]
    stream_nonce = boot["streamNonce"]
    token = boot.get("token")

    STATE = SidecarState(secret=secret, stream_nonce=stream_nonce, token=token)
    server = ThreadingHTTPServer(("127.0.0.1", 0), YamampHandler)
    port = server.server_address[1]
    sys.stdout.write(json.dumps({"port": port}) + "\n")
    sys.stdout.flush()

    try:
        server.serve_forever()
    except KeyboardInterrupt:
        pass
    finally:
        server.server_close()


if __name__ == "__main__":
    main()
