from __future__ import annotations
from collections import Counter
import typing
from threading import Lock

UUIDS: typing.List[str] = []
UUIDS_LOCK: Lock = Lock()


def print_uuid(slot: int) -> str:
    uuid: str = "MISSING"

    with UUIDS_LOCK:
        if slot < len(UUIDS):
            uuid = UUIDS[slot]

    return f"{uuid} (slot: {slot})"


def get_uuid(slot: int) -> str:
    uuid: str = "MISSING"

    with UUIDS_LOCK:
        if slot < len(UUIDS):
            uuid = UUIDS[slot]

    return uuid


class Event:
    def __init__(self):
        raise Exception("Base Event class should not be initialized")


class File:
    def __init__(self, id: str, path: str, size: int):
        self._id = id
        self._path = path
        self._size = size

    def __eq__(lhs, rhs):
        if not isinstance(rhs, File):
            return NotImplemented

        return lhs._id == rhs._id and lhs._path == rhs._path and lhs._size == rhs._size

    def __hash__(self):
        return hash(str(self._id))

    def __repr__(self):
        return f"File(id={self._id}, path={self._path}, size={self._size})"


class Queued(Event):
    def __init__(self, uuid_slot: int, peer: str, files: typing.Set[File]):
        self._uuid_slot = uuid_slot
        self._peer: str = peer
        self._files: typing.Set[File] = files

    def __eq__(self, rhs) -> bool:
        if not isinstance(rhs, Queued):
            return NotImplemented

        if (
            self._uuid_slot != rhs._uuid_slot
            or self._peer != rhs._peer
            or Counter(self._files) != Counter(rhs._files)
        ):
            return False

        return True

    def __str__(self):
        return f"Queued(peer={self._peer}, uuid={print_uuid(self._uuid_slot)}, files={self._files})"


class Receive(Event):
    def __init__(self, uuid_slot: int, peer: str, files: typing.Set[File]):
        self._uuid_slot: int = uuid_slot
        self._peer: str = peer
        self._files: typing.Set[File] = files

    def __eq__(self, rhs) -> bool:
        if not isinstance(rhs, Receive):
            return NotImplemented

        if (
            self._uuid_slot != rhs._uuid_slot
            or self._peer != rhs._peer
            or Counter(self._files) != Counter(rhs._files)
        ):
            return False

        return True

    def __str__(self):
        return f"Receive(peer={self._peer}, uuid={print_uuid(self._uuid_slot)}, files={self._files})"


class Start(Event):
    def __init__(
        self, uuid_slot: int, file: str, transferred: typing.Optional[int] = 0
    ):
        self._uuid_slot = uuid_slot
        self._file = file
        self._transferred = transferred

    def __eq__(self, rhs):
        if not isinstance(rhs, Start):
            return False
        if self._uuid_slot != rhs._uuid_slot:
            return False
        if self._file != rhs._file:
            return False

        if self._transferred is not None and rhs._transferred is not None:
            if self._transferred != rhs._transferred:
                return False

        return True

    def __str__(self):
        return f"Start(transfer={print_uuid(self._uuid_slot)}, file={self._file}, transfered={self._transferred})"


class Pending(Event):
    def __init__(self, uuid_slot: int, file: str):
        self._uuid_slot = uuid_slot
        self._file = file

    def __eq__(self, rhs):
        if not isinstance(rhs, Pending):
            return False
        if self._uuid_slot != rhs._uuid_slot:
            return False
        if self._file != rhs._file:
            return False

        return True

    def __str__(self):
        return f"Pending(transfer={print_uuid(self._uuid_slot)}, file={self._file})"


class Progress(Event):
    def __init__(
        self, uuid_slot: int, file: str, transferred: typing.Optional[int] = None
    ):
        self._uuid_slot = uuid_slot
        self._file = file
        self._transferred = transferred

    def __eq__(self, rhs) -> bool:
        if not isinstance(rhs, Progress):
            return False
        if self._uuid_slot != rhs._uuid_slot:
            return False
        if self._file != rhs._file:
            return False

        if self._transferred is not None and rhs._transferred is not None:
            if self._transferred != rhs._transferred:
                return False

        return True

    def __str__(self):
        return f"Progress(transfer={print_uuid(self._uuid_slot)}, file={self._file}, transfered={self._transferred})"


class Throttled(Event):
    def __init__(
        self, uuid_slot: int, file: str, transferred: typing.Optional[int] = 0
    ):
        self._uuid_slot = uuid_slot
        self._file = file
        self._transferred = transferred

    def __eq__(self, rhs):
        if not isinstance(rhs, Throttled):
            return False
        if self._uuid_slot != rhs._uuid_slot:
            return False
        if self._file != rhs._file:
            return False

        if self._transferred is not None and rhs._transferred is not None:
            if self._transferred != rhs._transferred:
                return False

        return True

    def __str__(self):
        return f"Throttled(transfer={print_uuid(self._uuid_slot)}, file={self._file}, transfered={self._transferred})"


class FinishTransferCanceled(Event):
    def __init__(self, uuid_slot: int, by_peer: bool):
        self._uuid_slot = uuid_slot
        self._by_peer = by_peer

    def __eq__(self, rhs):
        if not isinstance(rhs, FinishTransferCanceled):
            return False
        if self._uuid_slot != rhs._uuid_slot:
            return False
        if self._by_peer != rhs._by_peer:
            return False

        return True

    def __str__(self):
        return f"FinishTransferCanceled(transfer={print_uuid(self._uuid_slot)}, by_peer={self._by_peer})"


class FinishFileUploaded(Event):
    def __init__(self, uuid_slot: int, file: str):
        self._uuid_slot = uuid_slot
        self._file = file

    def __eq__(self, rhs):
        if not isinstance(rhs, FinishFileUploaded):
            return False
        if self._uuid_slot != rhs._uuid_slot:
            return False
        if self._file != rhs._file:
            return False

        return True

    def __str__(self):
        return f"FinishFileUploaded(transfer={print_uuid(self._uuid_slot)}, file={self._file})"


class FinishFileDownloaded(Event):
    def __init__(self, uuid_slot: int, file: str, final_path: str):
        self._uuid_slot = uuid_slot
        self._file = file
        self._final_path = final_path

    def __eq__(self, rhs):
        if not isinstance(rhs, FinishFileDownloaded):
            return False
        if self._uuid_slot != rhs._uuid_slot:
            return False
        if self._file != rhs._file:
            return False
        if self._final_path != rhs._final_path:
            return False

        return True

    def __str__(self):
        return f"FinishFileDownloaded(transfer={print_uuid(self._uuid_slot)}, file={self._file}, final_path={self._final_path})"


class FinishFileRejected(Event):
    def __init__(self, uuid_slot: int, file: str, by_peer: bool):
        self._uuid_slot = uuid_slot
        self._file = file
        self._by_peer = by_peer

    def __eq__(self, rhs):
        if not isinstance(rhs, FinishFileRejected):
            return False
        if self._uuid_slot != rhs._uuid_slot:
            return False
        if self._file != rhs._file:
            return False
        if self._by_peer != rhs._by_peer:
            return False

        return True

    def __str__(self):
        return f"FinishFileRejected(transfer={print_uuid(self._uuid_slot)}, file={self._file}, by_peer={self._by_peer})"


class Paused(Event):
    def __init__(self, uuid_slot: int, file: str):
        self._uuid_slot = uuid_slot
        self._file = file

    def __eq__(self, rhs):
        if not isinstance(rhs, Paused):
            return False
        if self._uuid_slot != rhs._uuid_slot:
            return False
        if self._file != rhs._file:
            return False

        return True

    def __str__(self):
        return f"Paused(transfer={print_uuid(self._uuid_slot)}, file={self._file})"


class ChecksumProgress(Event):
    def __init__(
        self, uuid_slot: int, file: str, checksummed_bytes: typing.Optional[int] = None
    ):
        self._uuid_slot = uuid_slot
        self._file = file
        self._checksummed_bytes = checksummed_bytes

    def __eq__(self, rhs):
        if not isinstance(rhs, ChecksumProgress):
            return False
        if self._uuid_slot != rhs._uuid_slot:
            return False
        if self._file != rhs._file:
            return False

        if self._checksummed_bytes is not None and rhs._checksummed_bytes is not None:
            if self._checksummed_bytes != rhs._checksummed_bytes:
                return False

        return True

    def __str__(self):
        return f"ChecksumProgress(transfer={print_uuid(self._uuid_slot)}, file={self._file}, checksummed_bytes={self._checksummed_bytes})"


class ChecksumStarted(Event):
    def __init__(self, uuid_slot: int, file: str, size: typing.Optional[int] = None):
        self._uuid_slot = uuid_slot
        self._file = file
        self._size = size

    def __eq__(self, rhs):
        if not isinstance(rhs, ChecksumStarted):
            return False
        if self._uuid_slot != rhs._uuid_slot:
            return False
        if self._file != rhs._file:
            return False
        if self._size is not None and rhs._size is not None:
            if self._size != rhs._size:
                return False

        return True

    def __str__(self):
        return f"ChecksumStarted(transfer={print_uuid(self._uuid_slot)}, file={self._file}), size={self._size}"


class ChecksumFinished(Event):
    def __init__(self, uuid_slot: int, file: str):
        self._uuid_slot = uuid_slot
        self._file = file

    def __eq__(self, rhs):
        if not isinstance(rhs, ChecksumFinished):
            return False
        if self._uuid_slot != rhs._uuid_slot:
            return False
        if self._file != rhs._file:
            return False

        return True

    def __str__(self):
        return f"ChecksumFinished(transfer={print_uuid(self._uuid_slot)}, file={self._file})"


class FinishFileFailed(Event):
    def __init__(
        self,
        uuid_slot: int,
        file: str,
        status: int,
        os_err: typing.Optional[int] = None,
    ):
        self._uuid_slot = uuid_slot
        self._file = file
        self._status = status
        self._os_err = os_err

    def __eq__(self, rhs):
        if not isinstance(rhs, FinishFileFailed):
            return False
        if self._uuid_slot != rhs._uuid_slot:
            return False
        if self._file != rhs._file:
            return False
        if self._status != rhs._status:
            return False
        if self._os_err != rhs._os_err:
            return False

        return True

    def __str__(self):
        return f"FinishFileFailed(transfer={print_uuid(self._uuid_slot)}, file={self._file}, status={self._status}, os_err={self._os_err})"


class FinishFailedTransfer(Event):
    def __init__(
        self,
        uuid_slot: int,
        status: int,
        os_err: typing.Optional[int] = None,
        ignore_os: bool = False,
    ):
        self._uuid_slot = uuid_slot
        self._status = status
        self._os_err = os_err
        self._ignore_os = ignore_os

    def __eq__(self, rhs):
        if not isinstance(rhs, FinishFailedTransfer):
            return False
        if self._uuid_slot != rhs._uuid_slot:
            return False
        if self._status != rhs._status:
            return False
        if not (self._ignore_os or rhs._ignore_os):
            if self._os_err != rhs._os_err:
                return False

        return True

    def __str__(self):
        return f"FinishFailedTransfer(transfer={print_uuid(self._uuid_slot)}, status={self._status}, os_err={self._os_err})"


class RuntimeError(Event):
    def __init__(self, status: int):
        self._status = status

    def __eq__(self, rhs):
        if not isinstance(rhs, RuntimeError):
            return False
        if self._status != rhs._status:
            return False
        return True

    def __str__(self):
        return f"RuntimeError(status={self._status})"


class TransferDeferred(Event):
    def __init__(
        self,
        uuid_slot: int,
        peer: str,
        status: int,
        os_err: typing.Optional[int] = None,
        ignore_os: bool = False,
    ):
        self._uuid_slot = uuid_slot
        self._peer = peer
        self._status = status
        self._os_err = os_err
        self._ignore_os = ignore_os

    def __eq__(self, rhs):
        if not isinstance(rhs, TransferDeferred):
            return False
        if self._uuid_slot != rhs._uuid_slot:
            return False
        if self._peer != rhs._peer:
            return False
        if self._status != rhs._status:
            return False
        if not (self._ignore_os or rhs._ignore_os):
            if self._os_err != rhs._os_err:
                return False

        return True

    def __str__(self):
        return f"TransferDeferred(transfer={print_uuid(self._uuid_slot)}, peer={self._peer}, status={self._status}, os_err={self._os_err})"
