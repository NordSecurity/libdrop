import asyncio
import ctypes
import json
import typing
import logging
from enum import IntEnum  # todo why two enums
from enum import Enum

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


class LogLevel(IntEnum):
    Critical = 1
    Error = 2
    Warning = 3
    Info = 4
    Debug = 5
    Trace = 6


LOG_LEVEL_MAP = {
    LogLevel.Critical: logging.CRITICAL,
    LogLevel.Error: logging.ERROR,
    LogLevel.Warning: logging.WARNING,
    LogLevel.Info: logging.INFO,
    LogLevel.Debug: logging.DEBUG,
    LogLevel.Trace: logging.DEBUG,
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


class EventQueue:
    def __init__(self):
        self._events: typing.List[event.Event] = []
        self._lock = Lock()

    def callback(self, ctx, s: str):
        if DEBUG_PRINT_EVENT:
            tprint(bcolors.HEADER + "--- event: ", s, bcolors.ENDC, flush=True)

        with self._lock:
            self._events.append(new_event(s))

    async def wait_for_any_event(self, duration: int, ignore_progress: bool = False):
        for _ in range(0, duration):
            with self._lock:
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
        self, target_event: event.Event, ignore_progress: bool = True
    ) -> None:
        # TODO: a better solution would be to have infinite loop with a timeout check for all wait commands
        for _ in range(100):
            with self._lock:
                while len(self._events) > 0:
                    e = self._events[0]
                    self._events = self._events[1:]

                    if ignore_progress and isinstance(e, event.Progress):
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
        self, target_events: typing.List[event.Event], ignore_progress: bool = True
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


class KeysCtx:
    def __init__(self, hostname: str):
        self.this = RUNNERS[hostname]

    def callback(self, ctx, ip, pubkey):
        ip = ip.decode("utf-8")
        peer = peer_resolver.reverse_lookup(ip)
        if peer is None:
            return 1

        found = RUNNERS[peer].pubkey

        ctypes.memmove(pubkey, found, len(found))
        return 0

    def secret(self):
        return self.this.privkey


class FdResolver:
    def __init__(self):
        self._files = []
        self._cached = {}

    def callback(self, ctx, uri):
        uri: str = uri.decode("utf-8")
        path = uri.removeprefix("content://")

        fd: int = -1
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
    def __init__(self, path: str, keys: KeysCtx):
        norddrop_lib = ctypes.cdll.LoadLibrary(path)

        logger_cb_func = ctypes.CFUNCTYPE(
            ctypes.c_void_p,
            ctypes.POINTER(ctypes.c_int),
            ctypes.POINTER(ctypes.c_char_p),
        )

        event_cb_func = ctypes.CFUNCTYPE(
            ctypes.c_void_p, ctypes.POINTER(ctypes.c_char_p)
        )

        pubkey_cb_func = ctypes.CFUNCTYPE(
            ctypes.c_int,
            ctypes.POINTER(ctypes.c_char_p),
            ctypes.POINTER(ctypes.c_char_p),
        )

        fd_cb_func = ctypes.CFUNCTYPE(
            ctypes.c_int,
            ctypes.POINTER(ctypes.c_char_p),
        )

        ceventtype = ctypes.CFUNCTYPE(None, ctypes.c_void_p, ctypes.c_char_p)
        clogtype = ctypes.CFUNCTYPE(
            None, ctypes.c_void_p, ctypes.c_int, ctypes.c_char_p
        )
        cpubkeytype = ctypes.CFUNCTYPE(
            ctypes.c_int,
            ctypes.c_void_p,
            ctypes.c_char_p,
            ctypes.POINTER(ctypes.c_char_p),
        )

        cfdresolvtype = ctypes.CFUNCTYPE(
            ctypes.c_int,
            ctypes.c_void_p,
            ctypes.c_char_p,
        )

        norddrop_lib.norddrop_version.restype = ctypes.c_char_p
        norddrop_lib.norddrop_new_transfer.restype = ctypes.c_char_p
        norddrop_lib.norddrop_get_transfers_since.restype = ctypes.c_char_p

        norddrop_lib.norddrop_set_peer_state.argtypes = (
            ctypes.c_void_p,
            ctypes.c_char_p,
            ctypes.c_int,
        )
        norddrop_lib.norddrop_start.argtypes = (
            ctypes.c_void_p,
            ctypes.c_char_p,
            ctypes.c_char_p,
        )
        norddrop_lib.norddrop_new_transfer.argtypes = (
            ctypes.c_void_p,
            ctypes.c_char_p,
            ctypes.c_char_p,
        )

        events = EventQueue()
        fdresolv = FdResolver()

        event_callback_wrapped = ceventtype(events.callback)
        log_callback_wrapped = clogtype(log_callback)
        pubkey_callback_wrapped = cpubkeytype(keys.callback)
        fd_callback_wrapped = cfdresolvtype(fdresolv.callback)

        class event_cb(ctypes.Structure):
            _fields_ = [("ctx", ctypes.c_void_p), ("cb", event_cb_func)]

            def __str__(self):
                return f"Python Event callback: ctx={self.ctx}, cb={self.cb}"

        class logger_cb(ctypes.Structure):
            _fields_ = [("ctx", ctypes.c_void_p), ("cb", logger_cb_func)]

            def __str__(self):
                return f"Python Logger callback: ctx={self.ctx}, cb={self.cb}"

        class pubkey_cb(ctypes.Structure):
            _fields_ = [("ctx", ctypes.c_void_p), ("cb", pubkey_cb_func)]

            def __str__(self):
                return f"Python Pubkey callback: ctx={self.ctx}, cb={self.cb}"

        class fdresolv_cb(ctypes.Structure):
            _fields_ = [("ctx", ctypes.c_void_p), ("cb", fd_cb_func)]

            def __str__(self):
                return f"Python FD resolver callback: ctx={self.ctx}, cb={self.cb}"

        logger_instance = logger_cb()
        eventer_instance = event_cb()
        pubkey_instance = pubkey_cb()
        fd_instance = fdresolv_cb()

        logger_instance.cb = ctypes.cast(log_callback_wrapped, logger_cb_func)
        eventer_instance.cb = ctypes.cast(event_callback_wrapped, event_cb_func)
        pubkey_instance.cb = ctypes.cast(pubkey_callback_wrapped, pubkey_cb_func)
        fd_instance.cb = ctypes.cast(fd_callback_wrapped, fd_cb_func)

        fake_instance = ctypes.pointer(ctypes.c_void_p())
        norddrop_instance = ctypes.pointer(fake_instance)

        norddrop_lib.norddrop_new(
            ctypes.pointer(norddrop_instance),
            eventer_instance,
            LogLevel.Trace,
            logger_instance,
            pubkey_instance,
            ctypes.create_string_buffer(keys.secret()),
        )

        norddrop_lib.norddrop_set_fd_resolver_callback(norddrop_instance, fd_instance)

        self._instance = norddrop_instance
        self._events = events
        self._fdresolv = fdresolv
        self._lib = norddrop_lib
        self._retain = [logger_instance, eventer_instance, pubkey_instance, fd_instance]

    def new_transfer(self, peer: str, descriptors: typing.List[str]) -> str:
        descriptors_json = []
        for descriptor in descriptors:
            descriptors_json.append({"path": descriptor})

        xfid = self._lib.norddrop_new_transfer(
            self._instance,
            ctypes.create_string_buffer(bytes(peer, "utf-8")),
            ctypes.create_string_buffer(bytes(json.dumps(descriptors_json), "utf-8")),
        )

        if xfid is None:
            raise DropException(
                "norddrop_new_transfer has failed to return a transfer ID"
            )

        return xfid.decode("utf-8")

    def new_transfer_with_fd(self, peer: str, path: str, uri: str) -> str:
        descriptor = [{"path": path, "content_uri": uri}]

        xfid = self._lib.norddrop_new_transfer(
            self._instance,
            ctypes.create_string_buffer(bytes(peer, "utf-8")),
            ctypes.create_string_buffer(bytes(json.dumps(descriptor), "utf-8")),
        )

        if xfid is None:
            raise DropException(
                "norddrop_new_transfer_with_fd has failed to return a transfer ID"
            )

        return xfid.decode("utf-8")

    def download(self, uuid: str, fid: str, dst: str):
        err = self._lib.norddrop_download(
            self._instance,
            ctypes.create_string_buffer(bytes(uuid, "utf-8")),
            ctypes.create_string_buffer(bytes(fid, "utf-8")),
            ctypes.create_string_buffer(bytes(dst, "utf-8")),
        )

        if err != 0:
            err_type = LibResult(err).name
            raise DropException(
                f"norddrop_download has failed with code: {err}({err_type})", err
            )

    def cancel_transfer_request(self, uuid: str):
        err = self._lib.norddrop_cancel_transfer(
            self._instance, ctypes.create_string_buffer(bytes(uuid, "utf-8"))
        )

        if err != 0:
            err_type = LibResult(err).name
            raise DropException(
                f"cancel_transfer_request has failed with code: {err}({err_type})", err
            )

    def cancel_transfer_file(self, uuid: str, fid: str):
        err = self._lib.norddrop_cancel_file(
            self._instance,
            ctypes.create_string_buffer(bytes(uuid, "utf-8")),
            ctypes.create_string_buffer(bytes(fid, "utf-8")),
        )

        if err != 0:
            err_type = LibResult(err).name
            raise DropException(
                f"cancel_file has failed with code: {err}({err_type})", err
            )

    def reject_transfer_file(self, uuid: str, fid: str):
        err = self._lib.norddrop_reject_file(
            self._instance,
            ctypes.create_string_buffer(bytes(uuid, "utf-8")),
            ctypes.create_string_buffer(bytes(fid, "utf-8")),
        )

        if err != 0:
            err_type = LibResult(err).name
            raise DropException(
                f"cancel_file has failed with code: {err}({err_type})", err
            )

    def get_transfers_since(self, since_timestamp: int) -> str:
        transfers = self._lib.norddrop_get_transfers_since(
            self._instance,
            ctypes.c_int64(since_timestamp),
        )

        if transfers is None:
            raise DropException(f"get_transfers_since has failed)")

        return transfers.decode("utf-8")

    def set_peer_state(self, peer: str, state: PeerState):
        addr = peer_resolver.resolve(peer)
        err = self._lib.norddrop_set_peer_state(
            self._instance,
            ctypes.create_string_buffer(bytes(addr, "utf-8")),
            state.value,
        )

        if err != 0:
            err_type = LibResult(err).name
            raise DropException(
                f"set_peer_state has failed with code: {err}({err_type})", err
            )

    def purge_transfers_until(self, until_timestamp: int):
        err = self._lib.norddrop_purge_transfers_until(
            self._instance,
            ctypes.c_int64(until_timestamp),
        )

        if err != 0:
            err_type = LibResult(err).name
            raise DropException(
                f"purge_transfers_until has failed with code: {err}({err_type})", err
            )

    def purge_transfers(self, xfids: typing.List[str]):
        err = self._lib.norddrop_purge_transfers(
            self._instance,
            ctypes.create_string_buffer(bytes(json.dumps(xfids), "utf-8")),
        )

        if err != 0:
            err_type = LibResult(err).name
            raise DropException(
                f"purge_transfers has failed with code: {err}({err_type})", err
            )

    def remove_transfer_file(self, uuid: str, fid: str):
        err = self._lib.norddrop_remove_transfer_file(
            self._instance,
            ctypes.create_string_buffer(bytes(uuid, "utf-8")),
            ctypes.create_string_buffer(bytes(fid, "utf-8")),
        )

        if err != 0:
            err_type = LibResult(err).name
            raise DropException(
                f"remove_transfer_file has failed with code: {err}({err_type})", err
            )

    def start(self, addr: str, dbpath: str):
        cfg = {
            "dir_depth_limit": 5,
            "transfer_file_limit": 1000,
            "moose_event_path": "/tmp/moose-events.json",
            "moose_app_version": "test-framework",
            "moose_prod": False,
            "storage_path": dbpath,
        }

        err = self._lib.norddrop_start(
            self._instance,
            ctypes.create_string_buffer(bytes(addr, "utf-8")),
            ctypes.create_string_buffer(bytes(json.dumps(cfg), "utf-8")),
        )
        if err != 0:
            err_type = LibResult(err).name
            raise DropException(
                f"norddrop_start has failed with code: {err}({err_type})", err
            )

    def stop(self):
        err = self._lib.norddrop_stop(self._instance)
        if err != 0:
            err_type = LibResult(err).name
            raise DropException(
                f"norddrop_stop has failed with code: {err}({err_type})", err
            )

    @property
    def version(self) -> str:
        version = self._lib.norddrop_version(self._instance)
        if not version:
            raise DropException(f"norddrop_version has failed")

        return ctypes.string_at(version).decode("utf-8")

    def __del__(self):
        err = self._lib.norddrop_destroy(self._instance)
        if err != 0:
            err_type = LibResult(err).name
            raise DropException(
                f"norddrop_destory has failed with code: {err}({err_type})", err
            )


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

    elif event_type == "TransferProgress":
        transfer = event_data["transfer"]

        with event.UUIDS_LOCK:
            transfer_slot = event.UUIDS.index(transfer)

        file = event_data["file"]
        progress: int = event_data["transfered"]

        return event.Progress(transfer_slot, file, progress)

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

    raise ValueError(f"Unhandled event received: {event_type}")


def log_callback(ctx, level, msg):
    msg = msg.decode("utf-8")
    logger.log(LOG_LEVEL_MAP.get(level), f"{msg}")
