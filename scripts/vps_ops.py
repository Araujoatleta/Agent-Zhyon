#!/usr/bin/env python3
"""Selective, bounded and transactional Hermes VPS backup/restore tooling."""
from __future__ import annotations

import argparse
import hashlib
import io
import json
import os
from pathlib import Path, PurePosixPath
import shutil
import stat
import sys
import tarfile
import tempfile
import time
from typing import Iterable

FORMAT_VERSION = 1
MAX_ARCHIVE_FILES = 10_000
MAX_ARCHIVE_FILE_SIZE = 256 * 1024 * 1024
MAX_ARCHIVE_TOTAL_SIZE = 2 * 1024 * 1024 * 1024
DEFAULT_INCLUDE = (
    "config.yaml", ".env", "auth.json", "skills", "plugins", "plugin-data",
    "mcp-tokens", "secrets", "platforms", "whatsapp", "memories", "memory",
    "cron", "hooks", "scripts", "profiles",
)
EXCLUDED_PARTS = {"cache", "caches", "logs", "sessions", "state", "gateway", "tools", "installs", "bin", "lsp", "backups", "node_modules", "target", "__pycache__"}
EXCLUDED_SUFFIXES = (".sock", ".lock", ".wal", ".shm")


class UnsafeArchive(ValueError): pass
class InvalidArchive(ValueError): pass


def _is_excluded(relative: Path) -> bool:
    return any(part.lower() in EXCLUDED_PARTS or "cache" in part.lower() for part in relative.parts) or relative.name.endswith(EXCLUDED_SUFFIXES) or relative.name.startswith("state.db")


def _contained(root: Path, candidate: Path) -> bool:
    try:
        candidate.relative_to(root)
        return True
    except ValueError:
        return False


def _reject_symlink_ancestors(source: Path, relative: Path) -> None:
    current = source
    for part in relative.parts:
        current /= part
        if current.is_symlink():
            raise ValueError(f"symlink in selected path: {relative}")


def _selected_files(source: Path, include: Iterable[str]) -> list[tuple[Path, Path]]:
    files: list[tuple[Path, Path]] = []
    for requested in include:
        relative = Path(requested)
        if relative.is_absolute() or not relative.parts or ".." in relative.parts:
            raise ValueError(f"unsafe include path: {requested}")
        _reject_symlink_ancestors(source, relative)
        candidate = source / relative
        if candidate.is_file() and not _is_excluded(relative):
            files.append((candidate, relative))
        elif candidate.is_dir():
            for item in sorted(candidate.rglob("*")):
                rel = item.relative_to(source)
                _reject_symlink_ancestors(source, rel)
                if item.is_file() and not _is_excluded(rel):
                    files.append((item, rel))
    return sorted(set(files), key=lambda pair: pair[1].as_posix())


def _digest(path: Path) -> str:
    value = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""): value.update(chunk)
    return value.hexdigest()


def create_backup(source: Path | str, output: Path | str, *, include: Iterable[str] = DEFAULT_INCLUDE,
                  remove: Iterable[str] = (), dry_run: bool = False, force: bool = False) -> dict:
    source = Path(source).resolve()
    output = Path(output).resolve()
    if not source.is_dir(): raise FileNotFoundError(f"source directory does not exist: {source}")
    if _contained(source, output): raise ValueError("backup output must not overlap the source tree")
    if output.exists() and not force: raise FileExistsError(f"output exists (use --force): {output}")
    files = _selected_files(source, include)
    removals = sorted(set(remove))
    for name in removals:
        p = PurePosixPath(name)
        if p.is_absolute() or not p.parts or ".." in p.parts: raise ValueError(f"unsafe removal path: {name}")
    manifest = {"format": FORMAT_VERSION, "created_at": int(time.time()), "files": {
        rel.as_posix(): {"sha256": _digest(path), "size": path.stat().st_size,
                         "mode": stat.S_IMODE(path.stat().st_mode)} for path, rel in files}, "remove": removals}
    result = {"archive": str(output), "files": len(files), "bytes": sum(v["size"] for v in manifest["files"].values()), "dry_run": dry_run}
    if dry_run: return result
    output.parent.mkdir(parents=True, exist_ok=True)
    fd, tmp_name = tempfile.mkstemp(prefix=output.name + ".", suffix=".tmp", dir=output.parent)
    os.close(fd); temporary = Path(tmp_name)
    try:
        with tarfile.open(temporary, "w:gz", format=tarfile.PAX_FORMAT) as archive:
            for path, relative in files:
                info = archive.gettarinfo(str(path), arcname=f"payload/{relative.as_posix()}")
                info.uid = info.gid = 0; info.uname = info.gname = ""
                info.mode = stat.S_IMODE(path.stat().st_mode) & 0o777
                with path.open("rb") as handle: archive.addfile(info, handle)
            raw = json.dumps(manifest, sort_keys=True, separators=(",", ":")).encode()
            info = tarfile.TarInfo("manifest.json"); info.size = len(raw); info.mode = 0o600; info.mtime = int(time.time())
            archive.addfile(info, io.BytesIO(raw))
        os.chmod(temporary, 0o600)
        if output.exists() and not force: raise FileExistsError(f"output exists (use --force): {output}")
        os.replace(temporary, output)
    finally: temporary.unlink(missing_ok=True)
    return result


def _safe_member(member: tarfile.TarInfo) -> None:
    name = member.name.replace("\\", "/"); path = PurePosixPath(name)
    if path.is_absolute() or not name or any(part in ("", ".", "..") for part in path.parts): raise UnsafeArchive(f"unsafe archive member: {member.name!r}")
    if member.issym() or member.islnk() or member.isdev() or member.isfifo(): raise UnsafeArchive(f"unsupported archive member type: {member.name!r}")
    if name != "manifest.json" and not name.startswith("payload/"): raise UnsafeArchive(f"unexpected archive member: {member.name!r}")


def validate_archive(archive_path: Path | str, *, max_files: int = MAX_ARCHIVE_FILES,
                     max_file_size: int = MAX_ARCHIVE_FILE_SIZE, max_total_size: int = MAX_ARCHIVE_TOTAL_SIZE) -> dict:
    try:
        with tarfile.open(Path(archive_path), "r:gz") as archive:
            members = archive.getmembers()
            for member in members: _safe_member(member)
            manifests = [m for m in members if m.name == "manifest.json" and m.isfile()]
            if len(manifests) != 1: raise InvalidArchive("archive must contain exactly one manifest.json")
            if manifests[0].size > 8 * 1024 * 1024: raise InvalidArchive("manifest is too large")
            raw = archive.extractfile(manifests[0]); manifest = json.loads(raw.read()) if raw else None
            if not isinstance(manifest, dict) or manifest.get("format") != FORMAT_VERSION or not isinstance(manifest.get("files"), dict): raise InvalidArchive("unsupported or malformed manifest")
            removals = manifest.get("remove", [])
            if not isinstance(removals, list): raise InvalidArchive("malformed removal list")
            actual = {m.name[8:]: m for m in members if m.isfile() and m.name.startswith("payload/")}
            if len(actual) > max_files: raise InvalidArchive("archive file count limit exceeded")
            if set(actual) != set(manifest["files"]): raise InvalidArchive("manifest file list does not match archive")
            total = 0
            for relative, metadata in manifest["files"].items():
                path = PurePosixPath(relative)
                if path.is_absolute() or not path.parts or ".." in path.parts: raise UnsafeArchive(f"unsafe manifest path: {relative!r}")
                member = actual[relative]; total += member.size
                if member.size > max_file_size: raise InvalidArchive(f"file size limit exceeded: {relative}")
                if total > max_total_size: raise InvalidArchive("archive total size limit exceeded")
                if not isinstance(metadata, dict) or metadata.get("size") != member.size: raise InvalidArchive(f"size mismatch: {relative}")
                extracted = archive.extractfile(member); digest = hashlib.sha256(extracted.read()).hexdigest() if extracted else ""
                if digest != metadata.get("sha256"): raise InvalidArchive(f"checksum mismatch: {relative}")
                mode = metadata.get("mode", member.mode & 0o777)
                if not isinstance(mode, int) or mode < 0 or mode > 0o777: raise InvalidArchive(f"invalid mode: {relative}")
            for relative in removals:
                path = PurePosixPath(relative)
                if not isinstance(relative, str) or path.is_absolute() or not path.parts or ".." in path.parts: raise UnsafeArchive(f"unsafe removal path: {relative!r}")
                if relative in actual: raise InvalidArchive(f"path cannot be both restored and removed: {relative}")
            return manifest
    except (tarfile.TarError, json.JSONDecodeError, KeyError, TypeError) as error:
        if isinstance(error, (UnsafeArchive, InvalidArchive)): raise
        raise InvalidArchive(f"invalid archive: {error}") from error


def _remove_path(path: Path) -> None:
    if path.is_dir() and not path.is_symlink(): shutil.rmtree(path)
    else: path.unlink(missing_ok=True)


def restore_backup(archive_path: Path | str, destination: Path | str, *, backup_dir: Path | str | None = None,
                   dry_run: bool = False, force_backup: bool = False) -> dict:
    archive_path, destination = Path(archive_path).resolve(), Path(destination).resolve()
    manifest = validate_archive(archive_path)
    backup_root = Path(backup_dir).resolve() if backup_dir else destination.parent / "backups"
    stamp = time.strftime("%Y%m%dT%H%M%SZ", time.gmtime()) + f"-{time.time_ns() % 1_000_000_000:09d}"
    destination_backup = backup_root / f"pre-restore-{stamp}.tgz"
    result = {"destination": str(destination), "destination_backup": str(destination_backup), "files": len(manifest["files"]), "dry_run": dry_run}
    if dry_run: return result
    backup_root.mkdir(parents=True, exist_ok=True); destination.mkdir(parents=True, exist_ok=True)
    incoming = tuple(manifest["files"].keys()) + tuple(manifest.get("remove", []))
    existing = [name for name in incoming if (destination / Path(*PurePosixPath(name).parts)).is_file()]
    missing = [name for name in incoming if not (destination / Path(*PurePosixPath(name).parts)).exists()]
    create_backup(destination, destination_backup, include=existing, remove=missing, force=force_backup)
    with tempfile.TemporaryDirectory(prefix="hermes-restore-", dir=destination.parent) as temp:
        staging = Path(temp) / "payload"; rollback = Path(temp) / "rollback"; staging.mkdir(); rollback.mkdir()
        with tarfile.open(archive_path, "r:gz") as archive:
            for member in archive.getmembers():
                if member.isfile() and member.name.startswith("payload/"):
                    relative = Path(*PurePosixPath(member.name[8:]).parts); target = staging / relative
                    target.parent.mkdir(parents=True, exist_ok=True); source = archive.extractfile(member)
                    if source is None: raise InvalidArchive(f"cannot read {member.name}")
                    with target.open("wb") as output: shutil.copyfileobj(source, output)
                    os.chmod(target, manifest["files"][member.name[8:]].get("mode", member.mode & 0o777))
        changed: list[tuple[Path, Path | None]] = []
        try:
            for relative in list(manifest["files"]) + list(manifest.get("remove", [])):
                rel = Path(*PurePosixPath(relative).parts); target = destination / rel
                _reject_symlink_ancestors(destination, rel.parent)
                saved = None
                if target.exists() or target.is_symlink():
                    saved = rollback / rel; saved.parent.mkdir(parents=True, exist_ok=True); os.replace(target, saved)
                changed.append((target, saved))
                if relative in manifest["files"]:
                    target.parent.mkdir(parents=True, exist_ok=True); os.replace(staging / rel, target)
        except BaseException:
            for target, saved in reversed(changed):
                _remove_path(target)
                if saved is not None:
                    target.parent.mkdir(parents=True, exist_ok=True); os.replace(saved, target)
            raise
    return result


def migrate(source: Path | str, destination: Path | str, *, backup_dir: Path | str | None = None, dry_run: bool = False) -> dict:
    if Path(source).resolve() == Path(destination).resolve(): raise ValueError("source and destination must differ")
    if dry_run:
        planned = create_backup(source, Path(destination).resolve().parent / "unused.tgz", dry_run=True)
        return {"backup": planned, "restore": {"destination": str(Path(destination).resolve()), "dry_run": True}}
    with tempfile.TemporaryDirectory(prefix="hermes-migrate-") as temp:
        archive = Path(temp) / "migration.tgz"; backup = create_backup(source, archive); restore = restore_backup(archive, destination, backup_dir=backup_dir)
        return {"backup": backup, "restore": restore}


def inspect_env(path: Path | str, required: Iterable[str] = ()) -> dict:
    names = {line.split("=", 1)[0].strip() for line in Path(path).read_text(encoding="utf-8").splitlines() if line.strip() and not line.lstrip().startswith("#") and "=" in line}
    required_set = set(required)
    return {"present": sorted(names & required_set), "missing": sorted(required_set - names), "defined_count": len(names)}


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__); parser.add_argument("--dry-run", action="store_true")
    sub = parser.add_subparsers(dest="command", required=True)
    backup = sub.add_parser("backup"); backup.add_argument("source", type=Path); backup.add_argument("output", type=Path); backup.add_argument("--force", action="store_true")
    check = sub.add_parser("validate"); check.add_argument("archive", type=Path)
    for name in ("restore", "rollback"):
        command = sub.add_parser(name); command.add_argument("archive", type=Path); command.add_argument("destination", type=Path); command.add_argument("--backup-dir", type=Path)
    move = sub.add_parser("migrate"); move.add_argument("source", type=Path); move.add_argument("destination", type=Path); move.add_argument("--backup-dir", type=Path)
    args = parser.parse_args(argv)
    try:
        if args.command == "backup": result = create_backup(args.source, args.output, dry_run=args.dry_run, force=args.force)
        elif args.command == "validate": result = {"archive": str(args.archive), "manifest": validate_archive(args.archive), "valid": True}
        elif args.command in ("restore", "rollback"): result = restore_backup(args.archive, args.destination, backup_dir=args.backup_dir, dry_run=args.dry_run)
        else: result = migrate(args.source, args.destination, backup_dir=args.backup_dir, dry_run=args.dry_run)
        print(json.dumps(result, sort_keys=True)); return 0
    except (OSError, ValueError) as error:
        print(f"error: {error}", file=sys.stderr); return 2


if __name__ == "__main__": raise SystemExit(main())
