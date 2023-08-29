import asyncio
from collections import Counter
from collections.abc import Iterable
from functools import reduce
import os, pwd
from pathlib import Path
import platform
import subprocess
import typing
import json
import time
import glob
import socket

from . import event, ffi
from .logger import logger
from .event import Event, print_uuid, get_uuid, UUIDS, UUIDS_LOCK

import sys


def compare_json_struct(expected: dict, actual: dict):
    for key in expected:
        if key is not int:
            if key not in actual:
                raise Exception(f"Key: '{key}' was not found in actual json struct")

        expected_value = expected[key]
        actual_value = actual[key]

        if isinstance(expected_value, dict):
            compare_json_struct(expected_value, actual_value)
        elif isinstance(expected_value, list):
            for i in range(len(expected_value)):
                compare_json_struct(expected_value[i], actual_value[i])
        else:
            # '*' is a special value that means that we don't care about the actual value of the output, just that it exists
            if expected_value == "*" and actual_value is not None:
                continue

            if expected_value != actual_value:
                raise Exception(
                    f"Value mismatch for key: '{key}'. Expected '{expected_value}', got '{actual_value}'"
                )


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


class ListenOnPort(Action):
    def __init__(self, addr: str):
        self._addr = addr
        self._socket: None | socket.socket = None
        pass

    async def run(self, drop: ffi.Drop):
        s = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
        s.bind((self._addr, 49111))
        s.listen()

        # prevent socket from being closed
        self._socket = s


class ExpectError(Action):
    def __init__(self, action: Action, err: int):
        self._action = action
        self._err = err

    async def run(self, drop: ffi.Drop):
        try:
            await self._action.run(drop)
        except ffi.DropException as e:
            if self._err != e.error():
                raise Exception(
                    f"Received DropException with error: {self._err}, expected {e.error()} instead"
                )
            else:
                return

        raise Exception(f"Action must have thrown DropException")


class Parallel(Action):
    def __init__(self, actions: typing.List[Action]):
        self._actions = actions

    async def run(self, drop: ffi.Drop):
        from concurrent.futures import ThreadPoolExecutor

        await asyncio.gather(*[action.run(drop) for action in self._actions])


class Repeated(Action):
    def __init__(self, actions: typing.List[Action], times: int):
        self._actions = actions
        self._times = times

    async def run(self, drop: ffi.Drop):
        print(f"Running {self._times} times")
        for _ in range(self._times):
            for action in self._actions:
                await action.run(drop)


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
        with UUIDS_LOCK:
            xfid = drop.new_transfer(self._peer, self._paths)
            UUIDS.append(xfid)

    def __str__(self):
        return f"NewTransfer({self._peer}, {self._paths})"


# New transfer just with files preopened. Used to test Android. Android can't share directories
# so this is limited to accept a single file
class NewTransferWithFD(Action):
    def __init__(self, peer: str, path: str, cached: bool = False):
        self._peer: str = peer
        self._path: str = path

        if cached:
            self._uri = f"content://cached{path}"
        else:
            self._uri = f"content://new{path}"

    async def run(self, drop: ffi.Drop):
        with UUIDS_LOCK:
            xfid = drop.new_transfer_with_fd(self._peer, self._path, self._uri)
            UUIDS.append(xfid)

    def __str__(self):
        return f"NewTransferWithFD({self._peer}, {self._uri})"


class Download(Action):
    def __init__(self, uuid_slot: int, fid, dst):
        self._uuid_slot = uuid_slot
        self._fid = fid
        self._dst = dst

    async def run(self, drop: ffi.Drop):
        with UUIDS_LOCK:
            drop.download(UUIDS[self._uuid_slot], self._fid, self._dst)  # TODO

    def __str__(self):
        return f"DownloadFile({print_uuid(self._uuid_slot)}, {self._fid}, {self._dst})"


class CancelTransferRequest(Action):
    def __init__(self, uuid_slot: int):
        self._uuid_slot = uuid_slot

    async def run(self, drop: ffi.Drop):
        with UUIDS_LOCK:
            drop.cancel_transfer_request(UUIDS[self._uuid_slot])

    def __str__(self):
        return f"CancelTransferRequest({print_uuid(self._uuid_slot)})"


class CancelTransferFile(Action):
    def __init__(self, uuid_slot: int, fid):
        self._uuid_slot = uuid_slot
        self._fid = fid

    async def run(self, drop: ffi.Drop):
        with UUIDS_LOCK:
            drop.cancel_transfer_file(UUIDS[self._uuid_slot], self._fid)

    def __str__(self):
        return f"CancelTransferFile({print_uuid(self._uuid_slot)}, {self._fid})"


class RejectTransferFile(Action):
    def __init__(self, uuid_slot: int, fid):
        self._uuid_slot = uuid_slot
        self._fid = fid

    async def run(self, drop: ffi.Drop):
        with UUIDS_LOCK:
            drop.reject_transfer_file(UUIDS[self._uuid_slot], self._fid)

    def __str__(self):
        return f"RejectTransferFile({print_uuid(self._uuid_slot)}, {self._fid})"


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


class SleepMs(Action):
    def __init__(self, ms: int):
        self._ms: int = ms

    async def run(self, drop: ffi.Drop):
        await asyncio.sleep(float(self._ms) / float(1000))

    def __str__(self):
        return f"SleepMs({self._ms})"


class Wait(Action):
    def __init__(self, event: Event):
        self._event: Event = event

    async def run(self, drop: ffi.Drop):
        await drop._events.wait_for(
            self._event, not isinstance(self._event, event.Progress)
        )

    def __str__(self):
        return f"Wait({str(self._event)})"


class WaitForOneOf(Action):
    def __init__(self, events: typing.List[Event]):
        self._events: typing.List[Event] = events

    async def run(self, drop: ffi.Drop):
        e = await drop._events.wait_for_any_event(100, ignore_progress=True)

        if e is None:
            raise Exception(f"Expected one of {self._events} but got nothing")

        if e not in self._events:
            raise Exception(f"Expected one of {self._events} but got {e}")

    def __str__(self):
        return f"WaitForOneOf({', '.join(str(e) for e in self._events)})"


class WaitRacy(Action):
    def __init__(self, events: typing.List[Event]):
        self._events: typing.List[Event] = events

    async def run(self, drop: ffi.Drop):
        await drop._events.wait_racy(
            self._events, not any(isinstance(ev, event.Progress) for ev in self._events)
        )

    def __str__(self):
        return f"WaitRacy({', '.join(str(e) for e in self._events)})"


class DrainEvents(Action):
    def __init__(self, count: int):
        self._count = count

    async def run(self, drop: ffi.Drop):
        for i in range(0, self._count):
            e = await drop._events.wait_for_any_event(100, ignore_progress=True)

            if e is None:
                raise Exception(f"Missing event number {i} while draining")

    def __str__(self):
        return f"DrainEvents({self._count})"


class NoEvent(Action):
    def __init__(self, duration: int = 3):
        self._duration = duration

    async def run(self, drop: ffi.Drop):
        e = await drop._events.wait_for_any_event(self._duration)
        if e is not None:
            raise Exception(
                f"Unexpected event: {str(e)} received while no event was expected"
            )

    def __str__(self):
        return f"NoEvent({self._duration})"


class AssertNoEventOfType(Action):
    def __init__(self, forbidden: typing.List[type], duration: int = 6):
        self._duration = duration
        self._forbidden = forbidden

    async def run(self, drop: ffi.Drop):
        evs = await drop._events.gather_all(self._duration)

        for ev in evs:
            for fev in self._forbidden:
                if isinstance(ev, fev):
                    raise Exception(
                        f"Unexpected event: {str(ev)} received while asserting no such event will happen"
                    )

    def __str__(self):
        return f"AssertNoEventOfType({self._duration}, {self._forbidden})"


# TODO: this should be split to only wait for one event
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
    def __init__(self, file_glob: str):
        self._file = file_glob

    async def run(self, drop: ffi.Drop):
        file_list = glob.glob(self._file)
        file = file_list[0]

        with open(file, "a") as f:
            f.write("42")

    def __str__(self):
        return f"ModifyFile({self._file})"


class DeleteFileFromFS(Action):
    def __init__(self, file_glob: str):
        self._file = file_glob

    async def run(self, drop: ffi.Drop):
        file_list = glob.glob(self._file)
        file = file_list[0]

        os.remove(file)

    def __str__(self):
        return f"DeleteFileFromFS({self._file})"


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
    def __init__(self, uuid_slot: int, file_id: str, tmp_file_path_glob: str):
        self._uuid_slot = uuid_slot
        self._file_id = file_id
        self._tmp_file_path = tmp_file_path_glob

    async def run(self, drop: ffi.Drop):
        file_list = glob.glob(self._tmp_file_path)
        stat = os.stat(file_list[0])  # just take the first find

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
    def __init__(self, file_path_glob: str):
        self._file_path = file_path_glob

    async def run(self, drop: ffi.Drop):
        file_list = glob.glob(self._file_path)
        for file in file_list:
            os.remove(file)

    def __str__(self):
        return f"DeleteFile({self._file_path})"


class AssertTransfers(Action):
    # offset the timestamp by 10 seconds to account for the time it takes for the test to run
    def __init__(
        self,
        expected_outputs: typing.List[str],
        since_timestamp: int = int(time.time() - 10),
    ):
        self._since_timestamp = since_timestamp
        self._expected_outputs = expected_outputs

    async def run(self, drop: ffi.Drop):
        transfers = json.loads(drop.get_transfers_since(self._since_timestamp))

        if len(transfers) != len(self._expected_outputs):
            raise Exception(
                f"Expected {len(self._expected_outputs)} transfer(s), got {len(transfers)}"
            )
        for i in range(len(self._expected_outputs)):
            compare_json_struct(json.loads(self._expected_outputs[i]), transfers[i])

    def __str__(self):
        return f"AssertTransfers({self._since_timestamp}, {','.join(self._expected_outputs)})"


class PurgeTransfersUntil(Action):
    def __init__(self, until_timestamp: int):
        self._until_timestamp = until_timestamp

    async def run(self, drop: ffi.Drop):
        drop.purge_transfers_until(self._until_timestamp)

    def __str__(self):
        return f"PurgeTransfersUntil({self._until_timestamp})"


class PurgeTransfers(Action):
    def __init__(self, uuid_indices: typing.List[int]):
        self.uuid_indices = uuid_indices

    async def run(self, drop: ffi.Drop):
        xfids = [get_uuid(i) for i in self.uuid_indices]
        drop.purge_transfers(xfids)

    def __str__(self):
        return f"PurgeTransfers({self.uuid_indices})"


class Start(Action):
    def __init__(self, addr: str, dbpath: str = ":memory:", max_reqs: int = 50):
        self._addr = addr
        self._dbpath = dbpath
        self._max_reqs = max_reqs

    async def run(self, drop: ffi.Drop):
        drop.start(self._addr, self._dbpath, self._max_reqs)

    def __str__(self):
        return f"Start(addr={self._addr}, dbpath={self._dbpath})"


class RemoveTransferFile(Action):
    def __init__(self, uuid_slot: int, fid):
        self._uuid_slot = uuid_slot
        self._fid = fid

    async def run(self, drop: ffi.Drop):
        with UUIDS_LOCK:
            drop.remove_transfer_file(UUIDS[self._uuid_slot], self._fid)

    def __str__(self):
        return f"RemoveTransferFile({print_uuid(self._uuid_slot)}, {self._fid})"


class AssertMooseEvents(Action):
    def __init__(
        self,
        expected_outputs: typing.List[str],
        events_file: str = "/tmp/moose-events.json",
    ):
        self._expected_outputs = expected_outputs
        self._events_file = events_file

    async def run(self, drop: ffi.Drop):
        if not os.path.exists(self._events_file):
            raise Exception(
                f"Moose events file not found at '{self._events_file}', maybe libdrop was built without `--features moose_file`?"
            )

        events = json.loads(open(self._events_file).read())

        if len(events) != len(self._expected_outputs):
            raise Exception(
                f"Expected {len(self._expected_outputs)} event(s), got {len(events)}"
            )
        for i in range(len(self._expected_outputs)):
            compare_json_struct(json.loads(self._expected_outputs[i]), events[i])

    def __str__(self):
        return f"AssertMooseEvents({','.join(self._expected_outputs)})"


class EnsureTakesNoLonger(Action):
    def __init__(self, action: Action, seconds: float):
        self._action = action
        self._secs = seconds

    async def run(self, drop: ffi.Drop):
        start = time.time()
        await self._action.run(drop)
        elapsed = time.time() - start

        if elapsed > self._secs:
            raise Exception(
                f"Action took longer ({elapsed} s) than expected ({self._secs} s)"
            )

    def __str__(self):
        return f"EnsureTakesNoLonger({self._action}, {self._secs})"
