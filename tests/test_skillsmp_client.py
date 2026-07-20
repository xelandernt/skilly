import io
import asyncio
import json
import tarfile
import threading
import time
from collections.abc import Iterator
from contextlib import contextmanager
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

import pytest

from skilly import Skill
from skilly.skillsmp.client import (
    AsyncSkillsMp,
    ClientSettings,
    SkillsMp,
    SkillsMpSearchQuery,
)


COMMIT_SHA = "0123456789abcdef0123456789abcdef01234567"


class SkillsServer(BaseHTTPRequestHandler):
    tarball: bytes = b""

    def do_GET(self) -> None:  # noqa: N802
        path = self.path.split("?", 1)[0]
        if self.path.startswith("/api/v1/skills/search"):
            self._json(
                {
                    "success": True,
                    "data": {
                        "skills": [
                            {
                                "id": "skill-1",
                                "name": "python-production",
                                "author": "idossha",
                                "description": "Python production code patterns.",
                                "githubUrl": "https://github.com/example/project/tree/main/skills/python",
                                "skillUrl": "https://skillsmp.com/skills/skill-1",
                                "stars": 42,
                                "updatedAt": "1778091502",
                            }
                        ],
                        "pagination": {
                            "page": 1,
                            "limit": 10,
                            "total": 1,
                            "totalPages": 1,
                            "hasNext": False,
                            "hasPrev": False,
                        },
                        "filters": {"search": "python"},
                    },
                }
            )
            return
        parts = [part for part in path.split("/") if part]
        if len(parts) == 5 and parts[0] == "repos" and parts[3] == "commits":
            self._json({"sha": COMMIT_SHA})
            return
        if len(parts) == 5 and parts[0] == "repos" and parts[3] == "tarball":
            self.send_response(200)
            self.send_header("Content-Type", "application/gzip")
            self.send_header("Content-Length", str(len(self.tarball)))
            self.end_headers()
            self.wfile.write(self.tarball)
            return
        self.send_response(404)
        self.end_headers()

    def log_message(self, format: str, *args) -> None:  # noqa: A003
        return None

    def _json(self, payload: object) -> None:
        body = json.dumps(payload).encode()
        self.send_response(200)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)


@contextmanager
def running_server(
    files: dict[str, str] | None = None,
) -> Iterator[tuple[str, ThreadingHTTPServer]]:
    SkillsServer.tarball = build_tarball_bytes(
        files
        or {
            "skills/python/SKILL.md": "---\nname: python\ndescription: Use python.\n---\nBody\n",
            "skills/python/scripts/extract.py": "print('hi')\n",
        }
    )
    server = ThreadingHTTPServer(("127.0.0.1", 0), SkillsServer)
    thread = threading.Thread(target=server.serve_forever, daemon=True)
    thread.start()
    try:
        address = server.server_address
        host = str(address[0])
        port = int(address[1])
        time.sleep(0.05)
        yield f"http://{host}:{port}", server
    finally:
        server.shutdown()
        thread.join()


def build_tarball_bytes(files: dict[str, str]) -> bytes:
    archive = io.BytesIO()
    with tarfile.open(fileobj=archive, mode="w:gz") as tar:
        for relative_path, content in files.items():
            encoded = content.encode()
            info = tarfile.TarInfo(f"example-project-{COMMIT_SHA[:7]}/{relative_path}")
            info.size = len(encoded)
            tar.addfile(info, io.BytesIO(encoded))
    return archive.getvalue()


def test_skillsmp_client_search_returns_a_repository_url(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    with running_server() as (server_url, _server):
        client = SkillsMp(settings=ClientSettings(base_url=f"{server_url}/api/v1"))

        response = client.search(SkillsMpSearchQuery(text="python", limit=10))

        assert response.data.skills[0].name == "python-production"
        assert response.data.skills[0].repository_url.endswith("/skills/python")
        assert response.data.pagination.total_pages == 1
        assert not hasattr(Skill, "from_github")
        assert not hasattr(Skill, "from_skillsmp")
        assert not hasattr(client, "fetch_github_snapshot")


def test_async_skillsmp_client_does_not_block_event_loop(monkeypatch) -> None:
    def slow_search(self, query):
        del self, query
        time.sleep(0.05)
        return "result"

    monkeypatch.setattr(SkillsMp, "search", slow_search)

    async def run() -> None:
        task = asyncio.create_task(AsyncSkillsMp().search("python"))
        await asyncio.sleep(0.01)
        assert not task.done()
        assert await task == "result"

    asyncio.run(run())
