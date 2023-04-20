import asyncio
import ctypes
import json
import typing
import logging
from enum import IntEnum

from . import event
from .logger import logger
from .config import RUNNERS


DEBUG_PRINT_EVENT = True

# fmt: off
PRIV_KEY = bytes([164, 70, 230, 247, 55, 28, 255, 147, 128, 74, 83, 50, 181, 222, 212, 18, 178, 162, 242, 102, 220, 203, 153, 161, 142, 206, 123, 188, 87, 77, 126, 183])
PUB_KEY = bytes([68, 103, 21, 143, 132, 253, 95, 17, 203, 20, 154, 169, 66, 197, 210, 103, 56, 18, 143, 142, 142, 47, 53, 103, 186, 66, 91, 201, 181, 186, 12, 136])
# fmt: on


class LibResult(IntEnum):
    # Operation was success
    NORDDROP_RES_OK = (0,)

    # Operation resulted to unknown error.
    NORDDROP_RES_ERROR = (1,)

    # Failed to marshal C string to Rust
    NORDDROP_RES_INVALID_STRING = (2,)

    # Bad JSON input
    NORDDROP_RES_BAD_CONFIG = (3,)


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

    def callback(self, ctx, s: str):
        if DEBUG_PRINT_EVENT:
            print("--- event: ", s, flush=True)
        self._events.append(new_event(s))

    async def wait_for_any_event(self, duration: int, ignore_progress: bool = False):
        for _ in range(0, duration):
            if ignore_progress:
                self._events = [
                    ev for ev in self._events if not isinstance(ev, event.Progress)
                ]

            if len(self._events) > 0:
                return self._events[0]

            await asyncio.sleep(1)

        return None

    async def wait_for(
        self, target_event: event.Event, ignore_progress: bool = True
    ) -> None:
        # TODO: a better solution would be to have infinite loop with a timeout check for all wait commands
        for _ in range(100):
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
            while len(self._events) > 0:
                e = self._events[0]
                self._events = self._events[1:]

                if ignore_progress and isinstance(e, event.Progress):
                    continue

                found = False
                for te in target_events:
                    if te == e:
                        found = True
                        success.append(te)
                        break

                if not found:
                    raise Exception(
                        f"Unexpected event1:\n{str(e)}\nwhile looking for(racy):\n{', '.join(str(e) for e in target_events if e not in success)}\n"
                    )

                i -= 1
                if len(success) == len(target_events):
                    return

            await asyncio.sleep(1)

        raise Exception(
            f"Events not received\nwhile looking for(racy), remained:\n{', '.join(str(e) for e in target_events if e not in success)}\n"
        )


class KeysCtx:
    def __init__(self, runner: str):
        self.this = RUNNERS[runner]

    def callback(self, ctx, ip, pubkey):
        found = None
        if ip is None:
            found = self.this.pubkey
        else:
            ip = ip.decode("utf-8")

            peer = None
            for pr in RUNNERS.values():
                if pr.ip == ip:
                    peer = pr
                    break

            if peer is None:
                return 1

            found = peer.pubkey

        ctypes.memmove(pubkey, found, len(found))
        return 0

    def secret(self):
        return self.this.privkey


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

        norddrop_lib.norddrop_version.restype = ctypes.c_char_p
        norddrop_lib.norddrop_new_transfer.restype = ctypes.c_char_p

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
        event_callback_wrapped = ceventtype(events.callback)
        log_callback_wrapped = clogtype(log_callback)
        pubkey_callback_wrapped = cpubkeytype(keys.callback)

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

        logger_instance = logger_cb()
        eventer_instance = event_cb()
        pubkey_instance = pubkey_cb()

        logger_instance.cb = ctypes.cast(log_callback_wrapped, logger_cb_func)
        eventer_instance.cb = ctypes.cast(event_callback_wrapped, event_cb_func)
        pubkey_instance.cb = ctypes.cast(pubkey_callback_wrapped, pubkey_cb_func)

        fake_instance = ctypes.pointer(ctypes.c_void_p())
        norddrop_instance = ctypes.pointer(fake_instance)

        norddrop_lib.norddrop_new(
            ctypes.pointer(norddrop_instance),
            eventer_instance,
            LogLevel.Debug,
            logger_instance,
            pubkey_instance,
            ctypes.create_string_buffer(keys.secret()),
        )

        self._instance = norddrop_instance
        self._events = events
        self._lib = norddrop_lib
        self._retain = [logger_instance, eventer_instance]

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
            raise Exception("norddrop_new_transfer has failed to return a transfer ID")

        return xfid.decode("utf-8")

    def new_transfer_with_fd(self, peer: str, path: str, fd: int) -> str:
        descriptor = [{"path": path, "fd": fd}]

        xfid = self._lib.norddrop_new_transfer(
            self._instance,
            ctypes.create_string_buffer(bytes(peer, "utf-8")),
            ctypes.create_string_buffer(bytes(json.dumps(descriptor), "utf-8")),
        )

        if xfid is None:
            raise Exception(
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
            raise Exception(
                f"norddrop_download has failed with code: {err}({err_type})"
            )

    def cancel_transfer_request(self, uuid: str):
        err = self._lib.norddrop_cancel_transfer(
            self._instance, ctypes.create_string_buffer(bytes(uuid, "utf-8"))
        )

        if err != 0:
            err_type = LibResult(err).name
            raise Exception(
                f"cancel_transfer_request has failed with code: {err}({err_type})"
            )

    def cancel_transfer_file(self, uuid: str, fid: str):
        err = self._lib.norddrop_cancel_file(
            self._instance,
            ctypes.create_string_buffer(bytes(uuid, "utf-8")),
            ctypes.create_string_buffer(bytes(fid, "utf-8")),
        )

        if err != 0:
            err_type = LibResult(err).name
            raise Exception(f"cancel_file has failed with code: {err}({err_type})")

    def start(self, addr: str, runner: str):
        cfg = {
            "dir_depth_limit": 5,
            "transfer_file_limit": 1000,
            "req_connection_timeout_ms": 10000,
            "connection_max_retry_interval_ms": 2000,
            "transfer_idle_lifetime_ms": 10000,
            "moose_event_path": "/tmp/moose-events",
            "moose_prod": False,
            "storage_path": f"/src/libdrop_{runner}.sqlite",
        }

        err = self._lib.norddrop_start(
            self._instance,
            ctypes.create_string_buffer(bytes(addr, "utf-8")),
            ctypes.create_string_buffer(bytes(json.dumps(cfg), "utf-8")),
        )
        if err != 0:
            err_type = LibResult(err).name
            raise Exception(f"norddrop_start has failed with code: {err}({err_type})")

    def stop(self):
        err = self._lib.norddrop_stop(self._instance)
        if err != 0:
            err_type = LibResult(err).name
            raise Exception(f"norddrop_stop has failed with code: {err}({err_type})")

    @property
    def version(self) -> str:
        version = self._lib.norddrop_version(self._instance)
        if not version:
            return ""

        return ctypes.string_at(version).decode("utf-8")


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


def _get_children(f: typing.Dict[str, typing.Any]) -> typing.Iterable[event.File]:
    if not f["children"]:
        return {}

    for k, v in f["children"].items():
        yield event.File(k, v["size"], set(_get_children(v)))


def new_event(event_str: str) -> event.Event:
    deserialized = json.loads(event_str)

    event_type = deserialized["type"]
    event_data = deserialized["data"]

    if event_type == "Panic":
        return event.Panic(event_data)

    elif event_type == "RequestReceived":
        transfer: str = event_data["transfer"]

        event.UUIDS_LOCK.acquire()
        transfer_slot: int = len(event.UUIDS)
        event.UUIDS.append(transfer)
        event.UUIDS_LOCK.release()

        return event.Receive(
            transfer_slot,
            event_data["peer"],
            {
                event.File(f["id"], f["size"], set(_get_children(f)))
                for f in event_data["files"]
            },
        )

    elif event_type == "TransferStarted":
        transfer = event_data["transfer"]

        event.UUIDS_LOCK.acquire()
        trasnfer_slot = event.UUIDS.index(transfer)
        event.UUIDS_LOCK.release()

        file: str = event_data["file"]

        return event.Start(trasnfer_slot, file)

    elif event_type == "TransferProgress":
        transfer = event_data["transfer"]

        event.UUIDS_LOCK.acquire()
        transfer_slot = event.UUIDS.index(transfer)
        event.UUIDS_LOCK.release()

        file = event_data["file"]
        progress: int = event_data["transfered"]

        return event.Progress(transfer_slot, file, progress)

    elif event_type == "TransferFinished":
        transfer = event_data["transfer"]

        event.UUIDS_LOCK.acquire()
        transfer_slot = event.UUIDS.index(transfer)
        event.UUIDS_LOCK.release()

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
            return event.FinishFailedTransfer(transfer_slot, data["status"])
        elif reason == "FileCanceled":
            return event.FinishFileCanceled(
                transfer_slot, data["file"], data["by_peer"]
            )
        elif reason == "FileFailed":
            return event.FinishFileFailed(transfer_slot, data["file"], data["status"])
        else:
            raise ValueError(f"Unexpected reason of {reason} for TransferFinished")

    elif event_type == "RequestQueued":
        transfer = event_data["transfer"]

        event.UUIDS_LOCK.acquire()
        transfer_slot = event.UUIDS.index(transfer)
        event.UUIDS_LOCK.release()

        return event.Queued(
            transfer_slot,
            {
                event.File(f["id"], f["size"], set(_get_children(f)))
                for f in event_data["files"]
            },
        )

    raise ValueError(f"Unhandled event received: {event_type}")


def log_callback(ctx, level, msg):
    msg = msg.decode("utf-8")
    logger.log(LOG_LEVEL_MAP.get(level), f"callback: {level}: {msg}")
