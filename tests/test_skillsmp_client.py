import io
import json
import tarfile
import threading
import time
from collections.abc import Iterator
from contextlib import contextmanager
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

import pytest

from skilly.skills import Skill, parse_github_skill_url
from skilly.skillsmp.client import SkillsMp


COMMIT_SHA = "0123456789abcdef0123456789abcdef01234567"


class SkillsServer(BaseHTTPRequestHandler):
    tarball: bytes = b""

    def do_GET(self) -> None:  # noqa: N802
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
        if self.path == "/repos/example/project/commits/main":
            self._json({"sha": COMMIT_SHA})
            return
        if self.path == f"/repos/example/project/tarball/{COMMIT_SHA}":
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
def running_server() -> Iterator[tuple[str, ThreadingHTTPServer]]:
    SkillsServer.tarball = build_tarball_bytes(
        {
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


def test_skillsmp_client_search_and_github_download(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    with running_server() as (server_url, _server):
        monkeypatch.setenv("SKILLY_GITHUB_API_BASE_URL", server_url)
        client = SkillsMp(base_url=f"{server_url}/api/v1")

        response = client.search("python")

        assert response.parsed_data.data.skills[0].name == "python-production"
        location = parse_github_skill_url(response.parsed_data.data.skills[0].githubUrl)
        assert client.resolve_github_ref_and_commit_sha(location) == (
            "main",
            COMMIT_SHA,
        )

        skill = Skill.from_github(client, response.parsed_data.data.skills[0].githubUrl)

        assert skill.name == "python"
        assert skill.github_commit_sha == COMMIT_SHA
        assert [resource.relative_path.as_posix() for resource in skill.resources] == [
            "scripts/extract.py"
        ]
