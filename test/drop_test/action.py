import asyncio
from collections import Counter
from functools import reduce
import os, pwd
from pathlib import Path
import platform
import subprocess
import typing

from . import event, ffi
from .logger import logger
from .event import Event, print_uuid, UUIDS, UUIDS_LOCK

import sys


class File:
    def __init__(self, path: str, size: int):
        self._path = path
        self._size = size

    def __eq__(lhs, rhs):
        if not isinstance(rhs, File):
            return NotImplemented

        return lhs._path == rhs._path and lhs._size == rhs._size

    def __hash__(self):
        return hash(str(self._path))

    def __repr__(self):
        return f"File(path={self._path}, size={self._size})"


class Action:
    def __init__(self):
        raise NotImplementedError("Base Action class should not be initialized")

    async def run(self, drop: ffi.Drop):
        raise NotImplementedError("run() on base Action class")


class NewTransferFails(Action):
    def __init__(self, peer: str, path: str):
        self._peer: str = peer
        self._path: str = path

    async def run(self, drop: ffi.Drop):
        try:
            xfid = drop.new_transfer(self._peer, [self._path])
        except:
            return

        raise Exception("NewTransferFails did not fail")

    def __str__(self):
        return f"NewTransferFails({self._peer}, {self._path})"


class NewTransfer(Action):
    def __init__(self, peer: str, paths: list[str]):
        self._peer: str = peer
        self._paths: list[str] = paths

    async def run(self, drop: ffi.Drop):
        UUIDS_LOCK.acquire()

        xfid = drop.new_transfer(self._peer, self._paths)
        UUIDS.append(xfid)

        UUIDS_LOCK.release()

    def __str__(self):
        return f"NewTransfer({self._peer}, {self._paths})"


# New transfer just with files preopened. Used to test Android. Android can't share directories
# so this is limited to accept a single file
class NewTransferWithFD(Action):
    def __init__(self, peer: str, path: str):
        self._peer: str = peer
        self._path: str = path
        self._fd: typing.Any = None

    async def run(self, drop: ffi.Drop):
        # save to object in order to increase the lifetime of the file until GC collects it and closes the file
        self._fd = open(self._path, "r")

        fo = self._fd.fileno()

        UUIDS_LOCK.acquire()

        xfid = drop.new_transfer_with_fd(self._peer, self._path, fo)
        UUIDS.append(xfid)

        UUIDS_LOCK.release()

    def __str__(self):
        return f"NewTransferWithFD({self._peer}, {self._path}, {self._fd})"


# Initiates multiple transfers with the same FD
class MultipleNewTransfersWithSameFD(Action):
    def __init__(self, peers: typing.List[str], path: str):
        self._peers = peers
        self._path: str = path
        self._fd: typing.Any = None

    async def run(self, drop: ffi.Drop):
        # save to object in order to increase the lifetime of the file until GC collects it and closes the file
        self._fd = open(self._path, "r")

        fo = self._fd.fileno()

        for peer in self._peers:
            UUIDS_LOCK.acquire()

            xfid = drop.new_transfer_with_fd(peer, self._path, fo)
            UUIDS.append(xfid)

            UUIDS_LOCK.release()

    def __str__(self):
        return (
            f"MultipleNewTransfersWithSameFD({self._peers}, {self._path}, {self._fd})"
        )


class Download(Action):
    def __init__(self, uuid_slot: int, fid, dst):
        self._uuid_slot = uuid_slot
        self._fid = fid
        self._dst = dst

    async def run(self, drop: ffi.Drop):
        UUIDS_LOCK.acquire()
        drop.download(UUIDS[self._uuid_slot], self._fid, self._dst)
        UUIDS_LOCK.release()

    def __str__(self):
        return f"DownloadFile({print_uuid(self._uuid_slot)}, {self._fid}, {self._dst})"


class CancelTransferRequest(Action):
    def __init__(self, uuid_slot: int):
        self._uuid_slot = uuid_slot

    async def run(self, drop: ffi.Drop):
        UUIDS_LOCK.acquire()
        drop.cancel_transfer_request(UUIDS[self._uuid_slot])
        UUIDS_LOCK.release()

    def __str__(self):
        return f"CancelTransferRequest({print_uuid(self._uuid_slot)})"


class CancelTransferFile(Action):
    def __init__(self, uuid_slot: int, fid):
        self._uuid_slot = uuid_slot
        self._fid = fid

    async def run(self, drop: ffi.Drop):
        UUIDS_LOCK.acquire()
        drop.cancel_transfer_file(UUIDS[self._uuid_slot], self._fid)
        UUIDS_LOCK.release()

    def __str__(self):
        return f"CancelTransferFile({print_uuid(self._uuid_slot)}, {self._fid})"


class CheckDownloadedFiles(Action):
    def __init__(self, files: typing.List[File]):
        self._files: typing.List[File] = files

    async def run(self, drop: ffi.Drop):
        for f in self._files:
            if not os.path.exists(f._path):
                logger.warn(f"File not found: {f._path}")
                raise Exception("File doesn't exist")

            size = os.path.getsize(f._path)
            expected = f._size
            if size != expected:
                raise Exception(
                    f"File sizes do not match. Found: {size} expected: {expected}"
                )

            if platform.system() == "Darwin":
                from osxmetadata import OSXMetaData, kMDItemWhereFroms

                md = OSXMetaData(f._path)

                if "meshnet" not in md.get(kMDItemWhereFroms):
                    raise Exception("Missing metadata item in downloaded file!")

    def __str__(self):
        return f"CheckDownloadedFiles({self._files})"


class CheckFileDoesNotExist(Action):
    def __init__(self, files: typing.List[str]):
        self._files: typing.List[str] = files

    async def run(self, drop: ffi.Drop):
        for f in self._files:
            if os.path.exists(f):
                logger.warn(f"File {f} should not exist")
                raise Exception("File exist")

    def __str__(self):
        return f"CheckFileDoesNotExist({self._files})"


class WaitForAnotherPeer(Action):
    def __init__(self):
        pass

    async def run(self, drop: ffi.Drop):
        await asyncio.sleep(2)

    def __str__(self):
        return f"WaitForAnotherPeer"


class Sleep(Action):
    def __init__(self, seconds: int):
        self._seconds: int = seconds

    async def run(self, drop: ffi.Drop):
        await asyncio.sleep(self._seconds)

    def __str__(self):
        return f"Sleep({self._seconds})"


class Wait(Action):
    def __init__(self, event: Event):
        self._event: Event = event

    async def run(self, drop: ffi.Drop):
        await drop._events.wait_for(
            self._event, not isinstance(self._event, event.Progress)
        )

    def __str__(self):
        return f"Wait({str(self._event)})"


class WaitRacy(Action):
    def __init__(self, events: typing.List[Event]):
        self._events: typing.List[Event] = events

    async def run(self, drop: ffi.Drop):
        await drop._events.wait_racy(self._events)

    def __str__(self):
        return f"WaitRacy({', '.join(str(e) for e in self._events)})"


class NoEvent(Action):
    def __init__(self, duration: int = 6):
        self._duration = duration

    async def run(self, drop: ffi.Drop):
        e = await drop._events.wait_for_any_event(self._duration)
        if e is not None:
            raise Exception(
                f"Unexpected event: {str(e)} received while no event was expected"
            )

    def __str__(self):
        return f"NoEvent({self._duration})"


class ExpectCancel(Action):
    def __init__(self, uuid_slots: typing.List[int], by_peer: bool):
        self._uuid_slots = uuid_slots
        self._by_peer = by_peer

    async def run(self, drop: ffi.Drop):
        events: typing.List[event.Event] = [
            event.FinishTransferCanceled(slot, self._by_peer)
            for slot in self._uuid_slots
        ]
        await drop._events.wait_racy(events, ignore_progress=False)

    def __str__(self):
        uuids = [print_uuid(slot) for slot in self._uuid_slots]
        return f"ExpectCancel({uuids})"


# Shape egress traffic. Slowing down and adding latency helps introducing
# determinism in the testing environment
class ConfigureNetwork(Action):
    def __init__(self, rate: str = "10mbit", latency: str = "3000ms"):
        self._rate = rate
        self._latency = latency

    async def run(self, drop: ffi.Drop):
        def ex(cmd: str):
            print(
                subprocess.run(
                    cmd.split(" "), stdout=subprocess.PIPE, stderr=subprocess.PIPE
                ),
                "<--",
                cmd,
                flush=True,
            )

        device = "eth0"
        ex(
            f"tc qdisc add dev {device} root tbf rate {self._rate} burst 64k latency {self._latency}"
        )
        ex(f"tc qdisc show dev {device}")

    def __str__(self):
        return f"ConfigureNetwork({self._rate}, {self._latency})"


class Stop(Action):
    def __init__(self):
        pass

    async def run(self, drop: ffi.Drop):
        drop.stop()

    def __str__(self):
        return "Stop"


class ModifyFile(Action):
    def __init__(self, file: str):
        self._file = file

    async def run(self, drop: ffi.Drop):
        with open(self._file, "a") as f:
            f.write("42")

    def __str__(self):
        return f"ModifyFile({self._file})"


class CompareTrees(Action):
    def __init__(self, out_dir: Path, tree: list[File]):
        self._out_dir = out_dir
        self._tree = tree

    async def run(self, drop: ffi.Drop):
        tree = [
            File(str(child.relative_to(self._out_dir)), child.stat().st_size)
            for child in self._out_dir.rglob("*")
            if child.is_file()
        ]

        if Counter(self._tree) != Counter(tree):
            raise Exception(
                f"Output directory content mismatch (got {tree}, expected {self._tree})"
            )

    def __str__(self):
        return f"CompareTrees({self._out_dir}, {self._tree})"


class WaitForResume(Action):
    def __init__(self, uuid_slot: int, file_id: str, tmp_file_path: str):
        self._uuid_slot = uuid_slot
        self._file_id = file_id
        self._tmp_file_path = tmp_file_path

    async def run(self, drop: ffi.Drop):
        stat = os.stat(self._tmp_file_path)
        await drop._events.wait_for(event.Start(self._uuid_slot, self._file_id), False)
        await drop._events.wait_for(
            event.Progress(self._uuid_slot, self._file_id, stat.st_size), False
        )

    def __str__(self):
        return f"WaitForResume({print_uuid(self._uuid_slot)}, {self._file_id}, {self._tmp_file_path})"


class DropPrivileges(Action):
    def __init__(self):
        pass

    async def run(self, drop: ffi.Drop):
        uid = pwd.getpwnam("nobody").pw_uid
        os.seteuid(uid)

    def __str__(self):
        return "DropPrivileges"


class DeleteFile(Action):
    def __init__(self, file_path: str):
        self._file_path = file_path

    async def run(self, drop: ffi.Drop):
        os.remove(self._file_path)

    def __str__(self):
        return f"DeleteFile({self._file_path})"
