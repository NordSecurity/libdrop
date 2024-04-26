import asyncio
import typing
import logging
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


LOG_LEVEL_MAP = {
    norddrop.LogLevel.CRITICAL: logging.CRITICAL,
    norddrop.LogLevel.ERROR: logging.ERROR,
    norddrop.LogLevel.WARNING: logging.WARNING,
    norddrop.LogLevel.INFO: logging.INFO,
    norddrop.LogLevel.DEBUG: logging.DEBUG,
    norddrop.LogLevel.TRACE: logging.DEBUG,
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

    def on_event(self, event: norddrop.Event):
        event = new_event(event)
        if DEBUG_PRINT_EVENT:
            tprint(bcolors.HEADER + "--- event: ", event, bcolors.ENDC, flush=True)

        with self._lock:
            self._events.append(event)

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
        ignore_finalize_checksum_progress: bool = True,
        ignore_verify_checksum_progress: bool = True,
    ) -> None:
        # TODO: a better solution would be to have infinite loop with a timeout check for all wait commands
        for _ in range(100):
            with self._lock:
                while len(self._events) > 0:
                    e = self._events[0]
                    self._events = self._events[1:]

                    if ignore_progress and isinstance(e, event.Progress):
                        continue

                    if ignore_finalize_checksum_progress and isinstance(
                        e, event.FinalizeChecksumProgress
                    ):
                        continue

                    if ignore_verify_checksum_progress and isinstance(
                        e, event.VerifyChecksumProgress
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
        ignore_finalize_checksum_progress: bool = True,
        ignore_verify_checksum_progress: bool = True,
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

                    if ignore_finalize_checksum_progress and isinstance(
                        e, event.FinalizeChecksumProgress
                    ):
                        continue

                    if ignore_verify_checksum_progress and isinstance(
                        e, event.VerifyChecksumProgress
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
        return norddrop.LogLevel.TRACE

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
        self._instance.finalize_transfer(uuid)

    def reject_transfer_file(self, uuid: str, fid: str):
        self._instance.reject_file(uuid, fid)

    def get_transfers_since(
        self, since_timestamp: int
    ) -> typing.List[norddrop.TransferInfo]:
        return self._instance.transfers_since(since_timestamp * 100)

    def network_refresh(self):
        self._instance.network_refresh()

    def purge_transfers_until(self, until_timestamp: int):
        self._instance.purge_transfers_until(until_timestamp * 1000)

    def purge_transfers(self, xfids: typing.List[str]):
        self._instance.purge_transfers(xfids)

    def remove_transfer_file(self, uuid: str, fid: str):
        self._instance.remove_file(uuid, fid)

    def start(
        self,
        addr: str,
        dbpath: str,
        checksum_events_size_threshold=None,
        checksum_events_granularity=None,
    ):
        cfg = norddrop.Config(
            dir_depth_limit=5,
            transfer_file_limit=1000,
            moose_event_path="/tmp/moose-events.json",
            moose_prod=False,
            storage_path=dbpath,
            checksum_events_size_threshold=checksum_events_size_threshold,
            checksum_events_granularity=checksum_events_granularity,
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


def new_event(ev: norddrop.Event) -> event.Event:
    # Transfer slot correction
    ev = ev.kind

    transfer_slot: int = 0
    if ev.is_request_received():
        with event.UUIDS_LOCK:
            transfer_slot = len(event.UUIDS)
            event.UUIDS.append(ev.transfer_id)
    elif hasattr(ev, "transfer_id"):
        with event.UUIDS_LOCK:
            transfer_slot = event.UUIDS.index(ev.transfer_id)

    # Peer correction
    if hasattr(ev, "peer"):
        ev.peer = peer_resolver.reverse_lookup(ev.peer)

    if ev.is_request_received():
        return event.Receive(transfer_slot, ev.peer, ev.files)
    elif ev.is_request_queued():
        return event.Queued(transfer_slot, ev.peer, ev.files)

    elif ev.is_file_started():
        return event.Start(transfer_slot, ev.file_id, ev.transfered)
    elif ev.is_file_progress():
        return event.Progress(transfer_slot, ev.file_id, ev.transfered)
    elif ev.is_file_downloaded():
        return event.FinishFileDownloaded(transfer_slot, ev.file_id, ev.final_path)
    elif ev.is_file_uploaded():
        return event.FinishFileUploaded(transfer_slot, ev.file_id)
    elif ev.is_file_failed():
        return event.FinishFileFailed(
            transfer_slot,
            ev.file_id,
            ev.status.status,
            ev.status.os_error_code,
        )
    elif ev.is_file_rejected():
        return event.FinishFileRejected(transfer_slot, ev.file_id, ev.by_peer)
    elif ev.is_file_paused():
        return event.Paused(transfer_slot, ev.file_id)
    elif ev.is_file_throttled():
        return event.Throttled(transfer_slot, ev.file_id, ev.transfered)
    elif ev.is_file_pending():
        return event.Pending(transfer_slot, ev.file_id)

    elif ev.is_transfer_finalized():
        return event.FinishTransferCanceled(transfer_slot, ev.by_peer)
    elif ev.is_transfer_failed():
        return event.FinishFailedTransfer(
            transfer_slot, ev.status.status, ev.status.os_error_code
        )
    elif ev.is_transfer_deferred():
        return event.TransferDeferred(
            transfer_slot, ev.peer, ev.status.status, ev.status.os_error_code
        )

    elif ev.is_finalize_checksum_progress():
        return event.FinalizeChecksumProgress(
            transfer_slot, ev.file_id, ev.bytes_checksummed
        )
    elif ev.is_finalize_checksum_started():
        return event.FinalizeChecksumStarted(transfer_slot, ev.file_id, ev.size)
    elif ev.is_finalize_checksum_finished():
        return event.FinalizeChecksumFinished(transfer_slot, ev.file_id)

    elif ev.is_verify_checksum_progress():
        return event.VerifyChecksumProgress(
            transfer_slot, ev.file_id, ev.bytes_checksummed
        )
    elif ev.is_verify_checksum_started():
        return event.VerifyChecksumStarted(transfer_slot, ev.file_id, ev.size)
    elif ev.is_verify_checksum_finished():
        return event.VerifyChecksumFinished(transfer_slot, ev.file_id)

    elif ev.is_runtime_error():
        return event.RuntimeError(ev.status)

    else:
        raise Exception("Unknown event type")


def log_callback(ctx, level, msg):
    msg = msg.decode("utf-8")
    logger.log(LOG_LEVEL_MAP.get(level), f"{msg}")
