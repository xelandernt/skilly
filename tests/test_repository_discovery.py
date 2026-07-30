import io
import json
import tarfile
from collections.abc import Mapping

import pytest

import skilly
from skilly import RepositoryDiscoveryClient


COMMIT_SHA = "0123456789abcdef0123456789abcdef01234567"


def _github_tarball() -> bytes:
    archive = io.BytesIO()
    with tarfile.open(fileobj=archive, mode="w:gz") as tar:
        content = b"---\nname: sample\ndescription: Sample skill.\n---\nBody\n"
        entry = tarfile.TarInfo(
            f"example-project-{COMMIT_SHA[:7]}/skills/sample/SKILL.md"
        )
        entry.size = len(content)
        tar.addfile(entry, io.BytesIO(content))
    return archive.getvalue()


class RecordingTransport:
    def __init__(self) -> None:
        self.requests: list[tuple[str, Mapping[str, str], Mapping[str, str]]] = []

    def get(
        self,
        url: str,
        *,
        headers: Mapping[str, str],
        params: Mapping[str, str],
    ) -> bytes:
        self.requests.append((url, headers, params))
        if url.endswith("/commits/main"):
            return json.dumps({"sha": COMMIT_SHA}).encode()
        if url.endswith(f"/tarball/{COMMIT_SHA}"):
            return _github_tarball()
        raise AssertionError(f"unexpected request: {url}")


def test_repository_discovery_uses_the_caller_transport_for_every_request() -> None:
    transport = RecordingTransport()

    skills = RepositoryDiscoveryClient(transport).discover(
        "https://github.com/example/project/tree/main/skills"
    )

    assert [skill.name for skill in skills] == ["sample"]
    assert [url for url, _, _ in transport.requests] == [
        "https://api.github.com/repos/example/project/commits/main",
        f"https://api.github.com/repos/example/project/tarball/{COMMIT_SHA}",
    ]
    assert all(
        headers["accept"] == "application/vnd.github+json"
        for _, headers, _ in transport.requests
    )
    assert all(params == {} for _, _, params in transport.requests)


def test_bitbucket_cloud_discovery_uses_the_caller_transport_for_tree_and_file_reads() -> (
    None
):
    class BitbucketTransport:
        def __init__(self) -> None:
            self.requests: list[str] = []

        def get(
            self,
            url: str,
            *,
            headers: Mapping[str, str],
            params: Mapping[str, str],
        ) -> bytes:
            del headers, params
            self.requests.append(url)
            if url.endswith("/commit/main"):
                return json.dumps({"hash": COMMIT_SHA}).encode()
            if url.endswith(f"/src/{COMMIT_SHA}/skills"):
                return json.dumps(
                    {
                        "values": [
                            {
                                "type": "commit_file",
                                "path": "skills/sample/SKILL.md",
                            }
                        ]
                    }
                ).encode()
            if url.endswith(f"/src/{COMMIT_SHA}/skills/sample/SKILL.md"):
                return b"---\nname: sample\ndescription: Sample skill.\n---\nBody\n"
            raise AssertionError(f"unexpected request: {url}")

    transport = BitbucketTransport()

    skills = RepositoryDiscoveryClient(transport).discover(
        "https://bitbucket.org/example/project/src/main/skills"
    )

    assert [skill.name for skill in skills] == ["sample"]
    assert transport.requests == [
        "https://api.bitbucket.org/2.0/repositories/example/project/commit/main",
        f"https://api.bitbucket.org/2.0/repositories/example/project/src/{COMMIT_SHA}/skills",
        f"https://api.bitbucket.org/2.0/repositories/example/project/src/{COMMIT_SHA}/skills/sample/SKILL.md",
    ]


def test_repository_discovery_propagates_transport_rejection() -> None:
    class RejectingTransport:
        def get(
            self,
            url: str,
            *,
            headers: Mapping[str, str],
            params: Mapping[str, str],
        ) -> bytes:
            del url, headers, params
            raise PermissionError("address denied")

    with pytest.raises(RuntimeError, match="address denied"):
        RepositoryDiscoveryClient(RejectingTransport()).discover(
            "https://github.com/example/project/tree/main/skills"
        )


def test_repository_discovery_is_not_a_module_level_shortcut() -> None:
    assert not hasattr(skilly, "discover_repository_skills")
