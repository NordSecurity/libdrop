import asyncio
import json
import typing
import logging
from enum import IntEnum  # todo why two enums
from enum import Enum
import bindings.norddrop as norddrop  # type: ignore

from threading import Lock

from . import event
from .logger import logger
from .config import RUNNERS
from .peer_resolver import peer_resolver

import datetime

DEBUG_PRINT_EVENT = True
from .colors import bcolors


class PeerState(Enum):
    Offline = 0
    Online = 1


def tprint(*args, **kwargs):
    timestamp = datetime.datetime.now().strftime("%Y-%m-%d %H:%M:%S")
    print(f"[{timestamp}]", *args, **kwargs)


class DropException(Exception):
    def __init__(self, msg, errno: None | int = None):
        super().__init__(msg)
        self._errno = errno

    def error(self) -> int | None:
        return self._errno


class LibResult(IntEnum):
    # Operation was success
    NORDDROP_RES_OK = (0,)

    # Operation resulted to unknown error.
    NORDDROP_RES_ERROR = (1,)

    # Failed to marshal C string to Rust
    NORDDROP_RES_INVALID_STRING = (2,)

    # Bad JSON input
    NORDDROP_RES_BAD_INPUT = (3,)

    # Bad JSON for API
    NORDDROP_RES_JSON_PARSE = (4,)

    # Failed to create a transfer
    NORDDROP_RES_TRANSFER_CREATE = (5,)

    # Instance not started
    NORDDROP_NOT_STARTED = (6,)

    # Address already in used
    NORDDROP_RES_ADDR_IN_USE = (7,)

    # Failed to start instance
    NORDDROP_RES_INSTANCE_START = (8,)

    # Failed to stop instance
    NORDDROP_RES_INSTANCE_STOP = (9,)

    # Invalid private key provided
    NORDDROP_RES_INVALID_PRIVKEY = (10,)

    # Database error
    NORDDROP_RES_DB_ERROR = (11,)


LOG_LEVEL_MAP = {
    norddrop.LogLevel.NORDDROP_LOG_CRITICAL: logging.CRITICAL,
    norddrop.LogLevel.NORDDROP_LOG_ERROR: logging.ERROR,
    norddrop.LogLevel.NORDDROP_LOG_WARNING: logging.WARNING,
    norddrop.LogLevel.NORDDROP_LOG_INFO: logging.INFO,
    norddrop.LogLevel.NORDDROP_LOG_DEBUG: logging.DEBUG,
    norddrop.LogLevel.NORDDROP_LOG_TRACE: logging.DEBUG,
}


class EventsDontMatch(Exception):
    def __init__(self, expected, received):
        super().__init__("Events don't match")

        # Now for your custom code...
        self.expected = expected
        self.received = received

    def __str__(self):
        return (
            f"EventsDontMatch\nExpected:\n{self.expected}\nReceived:\n{self.received}"
        )


class EventQueue(norddrop.EventCallback):
    def __init__(self):
        self._events: typing.List[event.Event] = []
        self._lock = Lock()

    def on_event(self, event: str):
        if DEBUG_PRINT_EVENT:
            tprint(bcolors.HEADER + "--- event: ", event, bcolors.ENDC, flush=True)

        with self._lock:
            self._events.append(new_event(event))

    async def wait_for_any_event(self, duration: int, ignore_progress: bool = False):
        for _ in range(0, duration):
            with self._lock:
                self._events = [
                    ev for ev in self._events if not isinstance(ev, event.Throttled)
                ]

                if ignore_progress:
                    self._events = [
                        ev for ev in self._events if not isinstance(ev, event.Progress)
                    ]

                if len(self._events) > 0:
                    e = self._events[0]
                    self._events = self._events[1:]
                    return e

            await asyncio.sleep(1)

        return None

    async def gather_all(self, duration: int):
        await asyncio.sleep(duration)

        with self._lock:
            evs = self._events
            self._events = []
            return evs

    async def wait_for(
        self,
        target_event: event.Event,
        ignore_progress: bool = True,
        ignore_checksum_progress: bool = True,
    ) -> None:
        # TODO: a better solution would be to have infinite loop with a timeout check for all wait commands
        for _ in range(100):
            with self._lock:
                while len(self._events) > 0:
                    e = self._events[0]
                    self._events = self._events[1:]

                    if ignore_progress and isinstance(e, event.Progress):
                        continue

                    if ignore_checksum_progress and isinstance(
                        e, event.ChecksumProgress
                    ):
                        continue

                    if e == target_event:
                        return

                    raise Exception(
                        f"Unexpected event:\n{str(e)}\nwhile looking for:\n{str(target_event)}\n"
                    )

            await asyncio.sleep(1)

        raise Exception(f"Events not received: {str(target_event)}")

    # expect all incoming events to be present in passed events in no
    # particular order
    async def wait_racy(
        self,
        target_events: typing.List[event.Event],
        ignore_progress: bool = True,
        ignore_throttled: bool = True,
        ignore_checksum_progress: bool = True,
    ) -> None:
        success = []

        i = 0

        # TODO: a better solution would be to have infinite loop with a timeout check for all wait commands
        while i < 100:
            i += 1

            with self._lock:
                while len(self._events) > 0:
                    e = self._events[0]
                    self._events = self._events[1:]

                    if ignore_progress and isinstance(e, event.Progress):
                        continue

                    if ignore_throttled and isinstance(e, event.Throttled):
                        continue

                    if ignore_checksum_progress and isinstance(
                        e, event.ChecksumProgress
                    ):
                        continue

                    found = False
                    for te in target_events:
                        if te == e:
                            success.append(te)
                            found = True
                            tprint(
                                f"*racy event({len(success)}/{len(target_events)}), found {e}",
                                flush=True,
                            )
                            break

                    if not found:
                        raise Exception(
                            f"Unexpected event:\n{str(e)}\nwhile looking for(racy):\n{''.join(str(e) + chr(10) for e in target_events if e not in success)}\n"
                        )

                    i -= 1
                    if len(success) == len(target_events):
                        return

            await asyncio.sleep(1)

        raise Exception(
            f"Events not received\nwhile looking for(racy), remained:\n{''.join(str(e) + chr(10) for e in target_events if e not in success)}\n"
        )

    async def clear(self):
        with self._lock:
            self._events = []


class KeysCtx(norddrop.KeyStore):
    def __init__(self, hostname: str):
        self.this = RUNNERS[hostname]

    def on_pubkey(self, peer: str) -> typing.Optional[bytes]:
        peer = peer_resolver.reverse_lookup(peer)
        if peer is None:
            return 1

        return RUNNERS[peer].pubkey

    def privkey(self) -> bytes:
        return self.this.privkey


class LogCallback(norddrop.Logger):
    def __init__(self):
        pass

    def level(
        self,
    ) -> norddrop.LogLevel:
        return norddrop.LogLevel.NORDDROP_LOG_TRACE

    def on_log(self, level: norddrop.LogLevel, msg: str):
        logger.log(level=LOG_LEVEL_MAP[level], msg=msg)


class FdResolver(norddrop.FdResolver):
    def __init__(self):
        self._files = []
        self._cached = {}

    def on_fd(self, uri) -> typing.Optional[int]:
        path = uri.removeprefix("content://")

        fd: typing.Optional[int] = None
        if path.startswith("new"):
            path = path.removeprefix("new")
            fd = self.open(path)
        elif path.startswith("cached"):
            path = path.removeprefix("cached")

            if path in self._cached:
                fd = self._cached[path]
            else:
                fd = self.open(path)
                self._cached[path] = fd
        else:
            raise Exception(f"Unknown content uri command: {uri}")

        return fd

    def open(self, path: str) -> int:
        file = open(path, "r")
        # save to object in order to increase the lifetime of the file until GC collects it and closes the file
        self._files.append(file)

        return file.fileno()


class Drop:
    def __init__(self, keys: KeysCtx):
        self._events = EventQueue()
        self._instance = norddrop.NordDrop(self._events, keys, LogCallback())
        self._instance.set_fd_resolver(FdResolver())

    def new_transfer(self, peer: str, paths: typing.List[str]) -> str:
        descriptors = []
        for descriptor in paths:
            descriptors.append(norddrop.TransferDescriptor.PATH(descriptor))

        return self._instance.new_transfer(peer, descriptors)

    def new_transfer_with_fd(self, peer: str, path: str, uri: str) -> str:
        descriptors = [
            norddrop.TransferDescriptor.FD(filename=path, content_uri=uri, fd=None)
        ]
        return self._instance.new_transfer(peer, descriptors)

    def download(self, uuid: str, fid: str, dst: str):
        self._instance.download_file(uuid, fid, dst)

    def cancel_transfer_request(self, uuid: str):
        self._instance.finish_transfer(uuid)

    def reject_transfer_file(self, uuid: str, fid: str):
        self._instance.reject_file(uuid, fid)

    def get_transfers_since(self, since_timestamp: int) -> str:
        return self._instance.transfers_since(
            norddrop.Timestamp.fromtimestamp(since_timestamp, datetime.timezone.utc)
        )

    def network_refresh(self):
        self._instance.network_refresh()

    def purge_transfers_until(self, until_timestamp: int):
        self._instance.purge_transfers_until(
            norddrop.Timestamp.fromtimestamp(until_timestamp, datetime.timezone.utc)
        )

    def purge_transfers(self, xfids: typing.List[str]):
        self._instance.purge_transfers(xfids)

    def remove_transfer_file(self, uuid: str, fid: str):
        self._instance.remove_file(uuid, fid)

    def start(self, addr: str, dbpath: str, checksum_events_size_threshold=None):
        cfg = norddrop.Config(
            dir_depth_limit=5,
            transfer_file_limit=1000,
            moose_event_path="/tmp/moose-events.json",
            moose_prod=False,
            storage_path=dbpath,
            checksum_events_size_threshold=checksum_events_size_threshold,
            connection_retries=1,
        )

        self._instance.start(addr, cfg)

    def stop(self):
        self._instance.stop()

    @property
    def version(self) -> str:
        return norddrop.version()


class IncomingRequestEntry:
    def __init__(self, id, path):
        self._id = id
        self._path = path

    def __str__(self):
        return f"IncomingRequestEntry(id={self._id}, path={self._path}"


class IncomingRequest:
    def __init__(
        self, peer: str, sender: str, txid: str, data: typing.List[IncomingRequestEntry]
    ):
        self._txid: str = txid
        self._peer: str = peer
        self._sender: str = sender
        self._data: typing.List[IncomingRequestEntry] = data

    def __str__(self):
        return f"IncomingRequest(txid={self._txid}, peer={self._peer}, sender={self._sender}, data={repr(self._data)})"


def new_event(event_str: str) -> event.Event:
    deserialized = json.loads(event_str)

    event_type = deserialized["type"]
    event_data = deserialized["data"]

    if event_type == "RequestReceived":
        transfer: str = event_data["transfer"]

        with event.UUIDS_LOCK:
            transfer_slot: int = len(event.UUIDS)
            event.UUIDS.append(transfer)

        return event.Receive(
            transfer_slot,
            peer_resolver.reverse_lookup(event_data["peer"]),
            {event.File(f["id"], f["path"], f["size"]) for f in event_data["files"]},
        )

    elif event_type == "TransferStarted":
        transfer = event_data["transfer"]

        with event.UUIDS_LOCK:
            trasnfer_slot = event.UUIDS.index(transfer)

        file: str = event_data["file"]
        transfered: int = event_data["transfered"]

        return event.Start(trasnfer_slot, file, transfered)

    elif event_type == "TransferPending":
        transfer = event_data["transfer"]

        with event.UUIDS_LOCK:
            trasnfer_slot = event.UUIDS.index(transfer)

        file = event_data["file"]

        return event.Pending(trasnfer_slot, file)

    elif event_type == "TransferProgress":
        transfer = event_data["transfer"]

        with event.UUIDS_LOCK:
            transfer_slot = event.UUIDS.index(transfer)

        file = event_data["file"]
        progress: int = event_data["transfered"]

        return event.Progress(transfer_slot, file, progress)

    elif event_type == "TransferThrottled":
        transfer = event_data["transfer"]

        with event.UUIDS_LOCK:
            transfer_slot = event.UUIDS.index(transfer)

        file = event_data["file"]
        progress = event_data["transfered"]

        return event.Throttled(transfer_slot, file, progress)

    elif event_type == "TransferFinished":
        transfer = event_data["transfer"]

        with event.UUIDS_LOCK:
            transfer_slot = event.UUIDS.index(transfer)

        reason = event_data["reason"]

        data = event_data["data"]

        if reason == "FileUploaded":
            return event.FinishFileUploaded(transfer_slot, data["file"])
        elif reason == "FileDownloaded":
            return event.FinishFileDownloaded(
                transfer_slot, data["file"], data["final_path"]
            )
        elif reason == "TransferCanceled":
            return event.FinishTransferCanceled(transfer_slot, data["by_peer"])
        elif reason == "TransferFailed":
            return event.FinishFailedTransfer(
                transfer_slot, data["status"], data.get("os_error_code")
            )
        elif reason == "FileFailed":
            return event.FinishFileFailed(
                transfer_slot, data["file"], data["status"], data.get("os_error_code")
            )
        elif reason == "FileRejected":
            return event.FinishFileRejected(
                transfer_slot, data["file"], data["by_peer"]
            )
        else:
            raise ValueError(f"Unexpected reason of {reason} for TransferFinished")

    elif event_type == "RequestQueued":
        transfer = event_data["transfer"]

        with event.UUIDS_LOCK:
            transfer_slot = event.UUIDS.index(transfer)

        return event.Queued(
            transfer_slot,
            peer_resolver.reverse_lookup(event_data["peer"]),
            {event.File(f["id"], f["path"], f["size"]) for f in event_data["files"]},
        )

    elif event_type == "TransferPaused":
        transfer = event_data["transfer"]

        with event.UUIDS_LOCK:
            transfer_slot = event.UUIDS.index(transfer)

        return event.Paused(transfer_slot, event_data["file"])

    elif event_type == "RuntimeError":
        status = event_data["status"]

        return event.RuntimeError(status)

    elif event_type == "ChecksumProgress":
        transfer = event_data["transfer"]

        with event.UUIDS_LOCK:
            transfer_slot = event.UUIDS.index(transfer)

        return event.ChecksumProgress(
            transfer_slot,
            event_data["file"],
            event_data["bytes_checksummed"],
        )

    elif event_type == "ChecksumStarted":
        transfer = event_data["transfer"]

        with event.UUIDS_LOCK:
            transfer_slot = event.UUIDS.index(transfer)

        return event.ChecksumStarted(
            transfer_slot,
            event_data["file"],
            event_data["size"],
        )

    elif event_type == "ChecksumFinished":
        transfer = event_data["transfer"]

        with event.UUIDS_LOCK:
            transfer_slot = event.UUIDS.index(transfer)

        return event.ChecksumFinished(
            transfer_slot,
            event_data["file"],
        )

    elif event_type == "TransferDeferred":
        transfer = event_data["transfer"]

        with event.UUIDS_LOCK:
            transfer_slot = event.UUIDS.index(transfer)

        return event.TransferDeferred(
            transfer_slot,
            peer_resolver.reverse_lookup(event_data["peer"]),
            event_data["status"],
            event_data.get("os_error_code"),
        )

    raise ValueError(f"Unhandled event received: {event_type}")


def log_callback(ctx, level, msg):
    msg = msg.decode("utf-8")
    logger.log(LOG_LEVEL_MAP.get(level), f"{msg}")
